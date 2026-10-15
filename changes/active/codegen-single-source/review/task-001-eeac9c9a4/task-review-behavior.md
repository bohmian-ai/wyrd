# TASK-001 independent behavior review

**Result: FAIL.** One public typing regression remains. No runtime, tenancy,
authorization, audit, or persisted-data change was found in the paths reviewed.

Subject: base `5bcd9e4872f1376f1722922fa63b67384243be04`, candidate
`eeac9c9a45b599b1727086ba30582d3f02b3230a`; HEAD remained the candidate.
The complete cumulative diff, approved revision-1 spec, original task and its
Execution Evidence were used. No sibling reviewer reports were read.

## Review Findings

### Important

**BEHAVIOR-001 — REGRESSION / INCORRECT: response projection loses actual
serialization presence and nullability.**

- **Obligations:** REQ-007, INV-003, INV-005, AC-006: correct wire optionality
  and nullability from Rust while preserving public precision.
- **Source:** `scripts/gen_ts_types.py:205–225`, particularly the response
  `default` heuristic at lines 215–220. The shared transient bundle in
  `crates/wyrd-spec/examples/gen_schemas.rs::contracts` carries the default
  schemars deserialization shape, which does not distinguish an omitted
  optional input from a field always serialized as `null` in a response.
- **Reachable caller/result:** `Workflow.run` in
  `sdks/wyrd-sdk-ts/wyrd/src/index.ts:1804` returns the generated `WorkflowRun`.
  `NativeWorkflow::run` (`native/src/workflow.rs:203–220`) calls the existing
  shared workflow and serializes its result through `NativeLifecycleResult`.
  The Rust owner `crates/wyrd-spec/src/card/workflow.rs:775–834` declares
  `WorkflowStepResult::{text,structured_output,started_at,ended_at,error}`
  and `WorkflowRun::{workflow,started_at,ended_at,error}` as `Option` fields
  with no `skip_serializing_if`. Serde always writes those keys, using null
  for `None`. Generated `card-types.ts:6655,6659,6671,6683` and
  `6718–6748` instead mark them optional. The previous public declaration
  correctly exposed these fields as required and nullable.
- **Sibling consequence:** the same ambiguity also affects genuinely omitted
  response fields in the opposite direction: `BifrostTableDescription`
  (`wyrd-spec/src/vala/api.rs:129–130`) skips
  `canonical_physical_fingerprint` when None; generated
  `card-types.ts:433` admits an additional null value. The changed type fixture
  now explicitly expects that widening. This is one producer-level issue,
  not separate fixes for workflow and Bifrost consumers.
- **Observable impact:** existing valid code such as
  `const ended: string | null = run.ended_at` and
  `const text: string | null = step.text` now fails strict TypeScript checking
  solely because the generator invents `undefined`. Consumers must add checks
  for values the runtime does not return; incomplete response fixtures are
  also accepted. The task evidence acknowledges this approximation, but the
  approved spec does not authorize a typing-precision exception.
- **Smallest correction boundary:** preserve response serialization presence
  and nullability at the existing Rust extraction/lowering boundary, and
  regenerate the existing module. Keep request optionality separate where the
  same Rust type is accepted as input. Use source-derived serde/schema
  information rather than handwritten SDK interfaces, field-name exception
  lists, downstream wrappers, or a new generation framework. Do not alter
  server serialization or annotate input contracts as required merely to fix
  output declarations.
- **Closure proof:** add focused compile-only assertions that the workflow
  fields above have exactly their existing required nullable types, and that
  a skipped Bifrost fingerprint is `string | undefined`, then run the existing
  TS typecheck and codegen checks. An ordinary owner-level regression for the
  lowerer's distinction is sufficient; no additional journey scenario is
  needed.

### Critical

None.

### Non-blocking notes

None. Optional redesigns and unrelated debt were excluded.

## Obligation acceptance matrix

| Obligation | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| REQ-001: retire committed JSON schemas and schema fixtures, retain UI examples | Diff deletes schema/fixture trees; `gen_schemas::main` writes only error codes and UI examples; existing UI import remains | Diff/inventory; implementation `codegen:check` claim | PASS |
| REQ-002: derive all five gateway MCP inputs and preserve catalog semantics | `mcp/gateway.rs::gated` substitutes `schema_of::<T>()`; descriptor strings/effects/permissions and handler owners retained; `principals::schema_of` retains local definitions | Implementation exact gateway-catalog test claim; source trace | PASS |
| REQ-003: remove schema appendices/dead generators while retaining docs index and enum agreement | `generate_llms_txt` retains sidebar-derived page index; inactive card renderer/schema inventory deleted; diagram test resolves same enum definitions from transient `WYRD_CONTRACTS` | Source/diff; implementation `docs:check` claim | PASS |
| REQ-004: native full-envelope retrieval, exact identity, empty registry aliases, stable hydrated aliases, no hydration/download | `PyCards::get` calls existing shared read, retains `Card`, pins UID, constructs `PyCardEnvelope` with empty aliases; hydrated construction validation retained; mapping/serialization getters retained | Updated existing eight-kind real-server journey preserves content checks; implementation RED/GREEN and integration claims | PASS |
| REQ-005: retain annotation conversion, public tool registration/invocation and Agent.as_tool | `_schema.py` removes only unused output conversion; decorator runtime body/registry invocation retained; added overloads describe existing callable signature | Diff/caller inspection; implementation 592-unit-test claim | PASS |
| REQ-006: one authored public stub tree, native shared declarations, pure-Python reexports, no Card replacement assignment | Templates/assembler removed; package-local stubs retained; moved Python definitions reexported from stub-less `_card`, `_session`, `_client`, `_policy`, `_telemetry`, `_commands`; root replacement removed | Authored validator source; independent py:lints/typecheck PASS | PASS |
| REQ-007: Rust-owned TS wire contracts, existing public names/coverage, exact fields/enums/optionality/nullability, fail unsupported lowering | Extended existing lowerer and Rust bundle replace registry/Bifrost/principals/gateway/workflow/operator copies; discriminated Cards and CardSummary timestamps/enums retained; response field presence/nullability is approximated | Existing type fixtures and tsc pass but do not assert lost precision; focused compiler probe reproduces BEHAVIOR-001 | FAIL |
| REQ-008: root/subpath declarations, Rust N-API generation, tsc wrapper declarations retained | Unchanged package `types`/exports retain `dist/index.d.ts` and otel declaration; generated Rust bridge retained | Source/package inspection; implementation ts:napi:check claim; independent tsc PASS | PASS |
| REQ-009: codegen/setup/docs/CI/rules close deleted references and retain useful stub validation/production separation | `mise.toml`, `pyproject.toml`, check_stubs.py, CI selection/workflows and authority updates close retired paths; codegen no longer touches .pyi; no feature/dependency change | Independent py:lints PASS; source/diff; implementation codegen/docs/CI claims | PASS |
| REQ-010: all language lints and public typechecks pass, one behavioral scenario plus focused checks | Existing lint/type lanes retained; single updated retrieval journey; no benchmark/provider/scenario framework added | Independent lints, py:lints, ts:lints, py:typecheck, direct tsc PASS; task records complete ts:typecheck | PASS |
| INV-001: server owners/contracts/validation/errors/auth/audit/tenancy/data unchanged, spec library remains pure | Runtime change limited to Python projection; gateway input derives reuse actual existing DTOs; Rust library changes remove snapshot-only tests; generation remains example tooling | Cumulative diff and caller trace | PASS |
| INV-002: only selected Python break, no compatibility/alternate getter, authoring holders retained, TS runtime unchanged | Generated Python model/export deletion and existing envelope reuse; other holder logic retained; TS changes are type imports/reexports and equivalent wire annotations | Retrieval/support/workflow consumer diffs; Python/TS runtime source comparison | PASS |
| INV-003: each API type declared once; precision preserved outside selected Python mapping | Type ownership consolidated, but generated TS response precision regresses | BEHAVIOR-001 | FAIL |
| INV-004: dependencies/features/boundaries retained; no new framework/transport/stub extractor/lifecycle/deleted-name check | Manifest/diff inspection; existing schemars/lowerer/N-API/tsc reused; new stub check extracts existing validator responsibilities | Cumulative source/diff | PASS |
| INV-005: do not weaken meaningful regressions, derives, drift, lints/typechecking | Removed Rust tests only compare deleted snapshots; serde/validation tests remain; TS optionality fixture accepts widening contrary to preserved precision | Rust test deletion audit; BEHAVIOR-001 and changed table type assertion | FAIL |
| Non-goals: no SDK feature/endpoints/Bifrost engine/Card kinds/migrations/deployment/publishing/unrelated UI | None added in cumulative diff; UI example import retained | Source/diff | PASS |
| AC-001: retired generated artifacts; UI examples survive | REQ-001 evidence | Implementation codegen claim and diff | PASS |
| AC-002: five direct Rust gateway schemas/catalog semantics | REQ-002 evidence | Exact catalog test recorded by task | PASS |
| AC-003: docs indexes/builds/diagram agreement without schema files | REQ-003 evidence | docs:check recorded by task | PASS |
| AC-004: single journey proves full native envelope/identity/alias behavior | Updated test_register_and_hydrate journey asserts native type, exact ref/kind, empty aliases, serialization and per-kind fields; hydrated path retained | Task RED/GREEN/integration claims; source assertions | PASS |
| AC-005: Python canonical declarations/exports/tool behavior | REQ-004/005/006 evidence | Independent py:lints/typecheck; task unit/integration claims | PASS |
| AC-006: Rust-derived precise TS contracts and declaration resolution | Names, generation and declaration ownership pass; serialization shape precision does not | BEHAVIOR-001; independent baseline tsc pass | FAIL |
| AC-007: all mandatory language lint lanes pass without reduced scope/new suppressions | Lint lane scope retained; py:lints adds retained stub checks | Independent verification.md: all three lint lanes PASS | PASS |
| AC-008: clean pipeline/packaging/authority/reproducible surviving outputs | REQ-009 evidence; source-fed TS/errors/UI/theme generation retained | Implementation codegen/docs/fmt/CI checks; independent static checks | PASS |

## Caller and abstraction audit

The existing `Cards::get`/`assert_selector_identity` owns registry selection and
identity verification. The candidate reuses that owner, rather than introducing
another resolver or graph hydration. `PyCardEnvelope` remains a projection over
the native Card; changing aliases does not relax the hydration constructor's
nonempty-alias invariant. Tests/support and workflow-loading consumers now use
the same envelope mappings.

Moved Bifrost, gateway policy, session, telemetry and CLI helper definitions
were compared with their previous public runtime owners. Native Bifrost calls
now import the bifrost native submodule directly; ownership, buffering,
cancellation and transport behavior remain in their original implementations.
The telemetry helper's schema selection adjustment retains supported mapping
and model-class inputs. The new stub validation script retains the prior
documentation/layout/export checks, without a second declaration tree.

TS wrapper ergonomics (camelCase options, RunningQuery projection, hydration
summaries owned by wyrd-client, and observation media/options) retain their
original owners. The material failure is in the shared wire projection, so a
consumer-level workaround would duplicate authority and fail to fix siblings.

## Verification Notes

The orchestrator's independent `verification.md` records passing Rust/Python/TS
lints, Python typecheck and direct TypeScript tsc against existing built native
declarations. The task records earlier successful runtime/unit/integration,
codegen, docs and format checks; no independent transcripts for those earlier
runs were supplied, and they were not represented as newly run here. No Cargo
commands were run by this reviewer.

A read-only TypeScript compiler-API probe was run through `mise exec -- node`,
using the installed `sdks/wyrd-sdk-ts/node_modules/typescript`. A virtual source
file imported current generated `WorkflowRun` and `WorkflowStepResult`, then
assigned `run.ended_at` and `step.text` to `string | null`. Both produced:
`Type 'string | null | undefined' is not assignable to type 'string | null'.`
No source or generated output was written. An initial probe used the wrong
workspace node_modules location and failed to load TypeScript; the corrected
probe above succeeded and supplied the two diagnostics.

Skipped fresh native/server builds and rerunning claimed runtime suites during
this immutable audit. Residual risk: recorded integration/codegen/docs results
were assessed through source and task evidence rather than independently rerun.
