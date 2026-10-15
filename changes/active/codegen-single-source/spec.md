---
id: SPEC-codegen-single-source
revision: 1
status: approved
---

# One source for SDK types and consumed generated outputs

## Objective and approval

Remove unused generated schemas, Python's second Card representation, and the
copied Python stub tree. Each language has one editable typing authority for
each API; generated projections have real consumers and no independent edits.

The user approved recording the preceding five-item recommendations through
their instruction to create a new spec and one implementation plan. They
explicitly retained `ui_problem_examples.json`, requested one implementation
scenario rather than a scenario matrix, and prioritized passing Rust, Python,
and TypeScript lints. This packet records that scope; it does not implement it.

## Required behavior

### 1. Remove persisted schema intermediates

- **REQ-001** Delete the checked-in generated JSON Schema files under
  `crates/wyrd-spec/schemas/` and the generated schema fixtures under
  `crates/wyrd-spec/tests/fixtures/{eval,trace,observation}/schemas/`. Stop their
  production. Keep `crates/wyrd-spec/schemas/ui_problem_examples.json` as
  generated UI example data, with its existing Rust error-catalog producer and
  direct UI consumer. This is not a ban on JSON payloads, authored fixtures,
  runtime schemas, OpenAPI, or source-level schema derives.
- **REQ-002** Keep the five gateway MCP input contracts available by deriving
  their schemas from the existing owning Rust types:
  `ProviderCredentialWrite`, `ProviderDeployment`, `GatewayFallbackPolicy`,
  `GatewayGovernancePolicy`, and `GatewayCapturePolicyWrite`. Remove their
  `include_str!` dependencies on generated files. Preserve tool names,
  descriptions, required fields, reference resolution, annotations, permission
  filtering, execution, and errors. Do not replace the files with handwritten
  schema objects or another copied contract.
- **REQ-003** Remove raw-schema appendices and schema-directory links from
  `docs/public/llms.txt` and `docs/public/llms-full.txt`; preserve the page index
  and valid documentation links. `generate_llms_txt.py::render_llms_full`
  currently appends the 15 kind schemas after the page index. No existing human
  documentation page needs those appendices. Delete the inactive
  `generate_card_docs.py` and its invocation; its destination pages do not
  exist. Remove the inactive schema-inventory rendering in
  `generate_api_docs.py` while preserving its other applicable behavior.
  Preserve meaningful diagram/server enum agreement using transient
  Rust-derived metadata instead of committed schemas. Retire only tests whose
  sole assertion is equality with a deleted generated schema snapshot.

### 2. Remove Python Card generation; retain callable tool schemas

- **REQ-004** Delete `_card_types.py`, its Python emitter, and the public
  symbols supplied solely by that module, including `Registered*`, its unions,
  and generated spec/metadata/status aliases. Python `Cards.get(card_ref)`
  returns the existing `wyrd.state.CardEnvelope`, also already exported as
  `wyrd.CardEnvelope`. Preserve exact identity and the complete server envelope
  through `card_ref`, `kind`, mapping-valued `metadata`, `spec`,
  `relationships`, optional `status`, `model_dump()`, and `model_dump_json()`.
  Kind uses the existing native `CardKind`. Registry retrieval downloads no
  artifacts and performs no hydration. Registry results have no graph aliases:
  `aliases` is `()`. Hydrated state retains its existing stable aliases and
  construction-time validation. No alias is fabricated from a Card name.
- **REQ-005** Keep `_schema.py::annotation_to_schema`, public `@tool`
  registration, Python callable invocation, and `Agent.as_tool` behavior.
  Delete unused `output_to_json_schema` and helpers/imports used only by it.
  Python annotation introspection remains runtime behavior, not a generated
  schema artifact or a substitute for Rust wire contracts.

### 3. One Python stub source

- **REQ-006** The package-local public `.pyi` files and narrowly scoped
  `_wyrd.pyi` become the directly authored native-API declarations. Delete
  `python/wyrd/stubs/`, the copy/import-rewrite assembly stage, generated-file
  banners on these now-authored stubs, and the obsolete `codegen:stubs` task
  and dependencies. Preserve declarations, documentation, overloads, defaults,
  and exports except the explicitly removed generated Card API. Shared native
  types are declared once and re-exported/imported elsewhere. Pure Python
  definitions use their inline annotations, including a single definition of
  runtime protocols such as `Card` and `SessionMemory`; stubs re-export those
  definitions rather than independently restating them. Remove the root
  `cards.Card` replacement assignment. Runtime `.py` modules remain required.

### 4. One TypeScript authority for each contract

- **REQ-007** Rust `wyrd-spec` types own every existing TypeScript SDK wire
  contract. Extend the existing schema lowering/TypeScript emitter to consume
  transient Rust-derived metadata and replace handwritten copies of those
  contracts in the SDK. This includes registry, Bifrost, principals, gateway,
  workflow, and operator-connection wire types that the SDK currently exposes;
  it does not add absent API capabilities. Preserve public names and runtime
  behavior, correcting mismatched fields, enum unions, optionality, and
  nullability to the Rust contract. In particular, `CardSummary` includes
  required `created_at`/`updated_at` and the owning enum types. Keep existing
  typed Card discrimination and supported spec coverage. Genuinely opaque
  JSON remains opaque; unsupported lowering must fail with a diagnostic rather
  than silently weaken a known contract to `any`, `unknown`, or an index
  signature. Transient metadata may use stdout or build/temp storage but is
  neither committed nor published as another schema tree.
- **REQ-008** Keep `dist/index.d.ts` as the TypeScript public root entry and
  declarations for existing exported subpaths. N-API declarations remain
  generated from Rust binding APIs; public wrapper declarations remain emitted
  from TypeScript source. Any required CJS declaration copy is a derived
  build artifact, never a second editable source. TypeScript-owned ergonomic
  APIs stay typed in their implementation. Do not flatten distinct module
  declarations into a giant manually maintained stub or redesign the N-API
  bridge to avoid type generation.

### 5. Close the pipeline and its rules

- **REQ-009** Update codegen, Python setup/packaging, docs generation, CI path
  selection, and their direct consumers so a clean checkout builds and checks
  without deleted artifacts. `codegen:regen` produces only consumed generated
  outputs; `codegen:check` still checks their reproducibility, including
  TypeScript wire types, error codes, UI examples, and existing theme tokens.
  It neither rewrites nor claims to regenerate authored Python stubs. Preserve
  useful existing stub documentation/layout/export validation against the
  canonical public files without a second declaration tree or validation
  framework. Update `AGENTS.md`, `architecture/agent-rules.md`, the Python and
  testing references, `TESTING.md`, and affected architecture/docs statements
  in the same change: authored Python stubs are editable; actually generated
  artifacts remain source-driven. Keep production/test-harness separation.
- **REQ-010** All three language lint lanes must run and pass:
  `mise run lints`, `mise run py:lints`, and `mise run ts:lints`. Python and
  TypeScript public typing must also pass their existing typecheck lanes.
  Use one behavioral implementation scenario plus focused existing regression
  and static checks; do not expand this cleanup into a new scenario matrix,
  benchmark, multi-provider exercise, or generation framework.

## Source authority and boundaries

| Surface | Sole editable authority | Derived or consuming surface |
| --- | --- | --- |
| Durable wire fields and errors | Rust owner types/catalog in `wyrd-spec` | Runtime HTTP/MCP schemas and SDK projections |
| Python native API declarations | Public package `.pyi` files, including `_wyrd.pyi` | Editors, type checker, package consumers |
| Pure Python API declarations | Inline-annotated Python definitions | Public imports and stub re-exports |
| TypeScript wire declarations | Rust contract types through existing generation | Consumed generated TypeScript module(s) |
| TypeScript ergonomic API | TypeScript implementation | Compiler-emitted public `.d.ts` |
| Node native API | Rust N-API binding declarations | Generated private binding `.d.ts`/CJS projection |
| UI problem examples | Rust error catalog/projection | Retained `ui_problem_examples.json` |

Handwritten Python stubs are an explicit typing authority, not evidence that
signatures have been extracted from Rust. There is no native stub extractor
in the current repository. Typechecking and runtime regression checks remain
necessary; passing lints alone does not prove native signature parity.

## Invariants, compatibility, and non-goals

- **INV-001** Rust server owners, typed wire fields, validation, stable errors,
  permissions, audit, tenant isolation, and durable data formats are unchanged.
  SDKs continue to consume `wyrd-client`; `wyrd-spec` stays IO-/async-/PyO3-free
  in its library. Existing tooling examples may perform build-time IO.
- **INV-002** The deliberate Python compatibility break is removal of the
  generated Card model and mapping-based `CardEnvelope` retrieval. Add no
  deprecated aliases, lazy imports, compatibility dataclasses, or alternate
  retrieval API. Existing native authoring holders and kind-specific registry
  operations retain their behavior. TypeScript typing corrections follow
  Rust and do not change the wire or runtime operation.
- **INV-003** Declare each language API type once in its owning typing source;
  other public locations re-export it. Derived build output is not an
  independently authored definition. Preserve public typing precision outside
  the explicitly selected mapping-based Python Card envelope.
- **INV-004** Preserve existing dependencies and feature boundaries; reuse
  schemars, the current lowerer, N-API, and tsc. No new dependency, generic
  generator framework, native stub extractor, transport, schema service,
  lifecycle owner, or permanent deleted-name checker is authorized.
- **INV-005** Do not weaken lints, typechecking, useful regression assertions,
  meaningful schema derivation, or surviving drift checks to make this pass.
  Removal of schema snapshot equality and template-copy drift checks is
  authorized because their outputs/duplicate authority are deliberately
  retired. It does not authorize removing validation, serde, error, MCP, or
  diagram-contract tests.

Out of scope: new SDK features, new public endpoints, Bifrost engine changes,
new Card kinds, server data migrations, deployment, publishing, Git operations,
and unrelated UI work. `ui_problem_examples.json` and its current import remain.

## Acceptance criteria and evidence

| ID | Required outcome | Owner and credible proof |
| --- | --- | --- |
| AC-001 | No committed generated JSON Schema intermediates or generated schema fixture trees remain; UI examples remain generated and consumed. | Contract tooling; inventory/diff inspection and `codegen:check`. |
| AC-002 | Five gateway MCP schemas derive from their owning Rust requests and retain catalog semantics. | Server MCP; existing focused gateway catalog regression. |
| AC-003 | Docs generation no longer reads schema files or publishes dead schema links; prose pages/index and diagram enum agreement survive. | Docs generators/tests; `docs:check`. |
| AC-004 | Generic Python Card retrieval returns `CardEnvelope` with full content, exact identity, and empty registry aliases; hydrated aliases remain valid. | Python SDK; update the existing real-server retrieval journey in the single implementation scenario. |
| AC-005 | Generated Python Card exports, duplicate stub templates, copy assembly, and obsolete dependencies are gone; canonical stubs/imports and callable tool behavior work. | Python SDK/tooling; static closure, public typing, unit regressions, and retained journey assertions. |
| AC-006 | Existing TypeScript wire types come from Rust without committed JSON inputs or handwritten duplicate fields; root/subpath/native declarations resolve. | TypeScript/codegen; regeneration, typecheck including type fixtures, existing unit regressions, and package declaration inspection. |
| AC-007 | Rust, Python, and TypeScript lint lanes all pass without reduced scope or new suppressions. | Implementation owner; recorded successful lint commands or their containing integrated gate. |
| AC-008 | Pipeline, packaging, CI references, and repository rules describe the new authorities; surviving outputs reproduce. | Tooling/authority owners; codegen/docs/type checks and final diff audit. |

The task lists focused commands and scoped checks. At integrated change review,
apply `AGENTS.md` §11 to this shared codegen/build change: run `mise run gate`
once, whose checked-in shards include all three required language lint lanes.
Do not repeat its component lanes as additional final aggregates. This does not
add planned behavioral scenarios; it retains the repository's integrated gate.

## Open material decisions

None. Private helpers, output module subdivision, transient metadata transport,
and fixture mechanics remain implementation-owned within these decisions.

## Revision history

| Revision | Status | Basis |
| --- | --- | --- |
| 1 | approved | User-directed capture of the five-item decisions, retained UI examples, one implementation task/scenario, and mandatory language lints. |

## Authority

- [AGENTS.md](../../../AGENTS.md), especially §§2, 8, 11, 12, and 14.
- [Agent rules](../../../architecture/agent-rules.md).
- [Wyrd design](../../../architecture/wyrd-design.md) and
  [doctrine](../../../architecture/wyrd-doctrine.mdx).
- [Spec-driven development](../../../architecture/references/languages/spec-driven-development.md),
  [implementation execution](../../../architecture/references/languages/implementation-execution.md),
  and [testing workflows](../../../architecture/references/languages/testing-workflows.md).
- [Python API and stubs](../../../architecture/references/languages/python-api-and-stubs.md)
  and [TypeScript guide](../../../architecture/references/languages/typescript-guide.md).

The authored-stub decision explicitly supersedes the current blanket
generated-Python-stub instructions. REQ-009 requires reconciling those rules
before this change is complete; unrelated generated artifacts stay protected.
