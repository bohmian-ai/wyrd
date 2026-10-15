# SDK contract projection domain review

Result: **FAIL**

Subject: base `5bcd9e4872f1376f1722922fa63b67384243be04` → candidate
`eeac9c9a45b599b1727086ba30582d3f02b3230a`. HEAD remained the candidate during
inspection; source was not edited. This independent review read no sibling
review reports.

## Boundary and authority coverage

| Boundary | Authority | Source and consumer coverage | Result |
| --- | --- | --- | --- |
| Rust contract → transient definitions → TypeScript wire declarations | Approved spec REQ-007, INV-001/003/004/005; `AGENTS.md` §§2,8,9; `architecture/wyrd-design.md` client model; TypeScript guide | `gen_schemas.rs` shared `SchemaGenerator` and roots; entire `gen_ts_types.py`; generated `card-types.ts`; `src/index.ts` imports/public aliases and return paths; type fixtures | FAIL: SDK-001 |
| Existing discriminated Cards and definition collisions | REQ-007/008; typed spec coverage and public-name preservation | Supported/deferred Card lowering, `fresh`/`require`/`drain`, CardRef/spec/status references, `FieldSpec` versus Bifrost `FieldSpec2`, public `FieldDescription` alias; base/current exported generated-name comparison | PASS within inspected boundary: no old generated export name disappeared; current Bifrost alias resolves to the correct field type, while Card `FieldSpec` remains separate |
| Native Python Card retrieval → public envelope | REQ-004; deliberate mapping-based break in INV-002 | `PyCards::get`, `PyCardEnvelope` getters/serialization, shared `Cards::get`/get-response validation boundary, package root/cards/state imports and stubs; retrieval/workflow/state integration consumers and public-surface tests | PASS by source: complete native Card retained, native kind, requested exact reference pinned to returned UID, no graph alias, no artifact download added; hydrated alias validation remains in construction |
| Native Python declarations and inline Python declarations | REQ-006, INV-003; Python API/stubs and PyO3 references | Base-to-candidate public `.pyi` diffs, narrow `_wyrd.pyi`, new `_card`, `_session`, `_client`, `_policy`, `_telemetry`, `_commands` owners; public imports and overloads | PASS by source: native declarations remain authored once; moved pure Python definitions are re-exported; Bifrost sql overloads and decorator ParamSpec precision survive |
| Python callable tool schema retention | REQ-005 | `_schema.py` retained annotation conversion, `agent/tool.py` public overloads/registration/callable path; unchanged Agent tool behavior and Skald prompt boundary diff | PASS by source: removed output helper does not replace runtime annotation introspection |
| Public TypeScript declaration boundary | REQ-008 | Package `types` and exports, generated native declaration import, root compiler-emitted wrapper exports, `otel` subpath | PASS by source; no second authored root declaration introduced |

Additional authority read: task specification and original task/evidence,
`architecture/agent-rules.md`, reference router, spec-driven development and
maintainer references, and applicable doctrine/client-model anchors. CodeGraph
was attempted before source searches; its two exploration calls provided little
relevant indexing coverage, so the review continued with direct source/diff
inspection. No Bifrost engine, storage, authorization, or tenant behavior was
changed by the inspected projection paths.

## Material finding

### SDK-001 — Response presence and nullability are weakened by deserialization schema metadata

- Classification: **INCORRECT / REGRESSION** (public contract; blocking).
- Violated obligation: REQ-007 requires correcting optionality/nullability to
  the owning Rust contract, and INV-003 preserves existing public typing
  precision outside the selected Python Card break. INV-005 does not authorize
  replacing an accurate surviving type assertion with a weaker one.
- Changed cause: `scripts/gen_ts_types.py:179-190,205-220` uses one default
  Schemars generator and tries to recover response presence by requiring only
  schema-required properties plus properties with a `default`. This does not
  distinguish serialized `Option<T>` fields from deserializable missing fields.
  The current metadata also leaves null in skip-none response fields.
- Exact affected outputs include `card-types.ts:6647-6685` (`WorkflowRun`),
  `:6718-6752` (`WorkflowStepResult`), `:568-593`
  (`CardRegistrationOutcome`), and `:427-448`
  (`BifrostTableDescription`). Sibling response consumers include registry
  list/registration, principal pages/credential metadata, and provider
  credential views; their Option fields pass through the same lowering.
- Producer/consumer evidence:
  1. `crates/wyrd-spec/src/card/workflow.rs:775-840` derives Serialize for
     step/run snapshots. `text`, `started_at`, `ended_at`, `error`, and
     `workflow` have no `skip_serializing_if`: None is an explicit JSON null,
     and the keys are always present. `sdks/wyrd-sdk-ts/native/src/workflow.rs`
     `NativeWorkflow::run` returns `NativeLifecycleResult::outcome(run...)`;
     `native/src/lib.rs:181-185` serializes the Rust value directly.
     `wyrd/src/index.ts:1804-1805` decodes it as `WorkflowRun` without a shape
     transformation. The baseline public types required these keys; generated
     types now add `undefined` through optional properties.
  2. `registry/submission.rs:102-118`,
     `auth/tenant_principals.rs:224-250`, and
     `gateway/credential.rs:192-212` likewise serialize nullable response
     fields without skipping them; their generated response declarations
     receive the same unsupported optionality.
  3. Conversely, `crates/wyrd-spec/src/vala/api.rs:125-149` explicitly skips
     None for `canonical_physical_fingerprint`,
     `compaction_target_file_size_bytes`, and `compaction_type`. A serialized
     field is therefore a non-null underlying value or absent, never null.
     The previous `TableDescription` declaration captured this; generation
     now includes null. `tests/types/table-description.test-d.ts:12` was
     changed from `string | undefined` to `string | null | undefined`, making
     that regression pass. `Bifrost.describeTable` at `index.ts:1341-1342`
     returns this untransformed Rust response.
- Observable consequence: existing valid consumer code such as
  `const started: string | null = run.started_at` now fails compilation because
  the SDK claims an impossible missing value. Callers of skip-none response
  fields must also accommodate impossible nulls; the generated type no longer
  states the actual server contract.
- Focused independent reproduction: a temporary `.mts` file outside the
  checkout imported the current generated types and contained the assignment
  above plus `// @ts-expect-error` before assigning null to
  `BifrostTableDescription["canonical_physical_fingerprint"]`. Running
  `mise exec -- pnpm --dir sdks/wyrd-sdk-ts/wyrd exec tsc --noEmit --strict --skipLibCheck --module NodeNext --target ES2023 <temporary-file>`
  produced TS2322 (unexpected undefined) and TS2578 (unexpectedly accepted
  null). The file was removed; no tracked file changed. The shell cleanup
  returned zero after the compiler diagnostics, so those diagnostics, rather
  than its wrapper exit status, are the evidence.
- Smallest correction boundary: preserve accurate serialization shape in the
  existing Rust-owned metadata and existing lowerer; do not add TypeScript
  field-name overrides, a second contract tree, or new dependencies. The
  installed Schemars 0.8 `SchemaSettings` has no direction/serialization-contract
  mode. Its derive source confirms that `#[schemars(required)]` alone strips
  Option nullability, so that is not a valid blanket fix. A bounded existing-
  mechanism solution for these response-only DTOs is explicit
  `#[serde(default)]` on always-serialized Option fields (missing Option already
  deserializes to None; this exposes default:null to the current output rule),
  and existing `#[schemars(with = "<underlying type>")]` annotations on
  skip-none response fields while retaining their serde default/skip rules
  (optional, non-null when present). Audit the sibling response-only roots
  under the same cause, preserving request-root accepted optional/null inputs
  and shared Card authoring behavior. Do not apply a global Option-null
  setting: that would break genuinely nullable fields and request contracts.
- Closure proof: strengthen the existing workflow type fixture to require
  `WorkflowRun["started_at"]` and `WorkflowStepResult["text"]` exactly
  `string | null`, retain a representative required-nullable registration or
  principal response assertion, and restore the existing table fingerprint
  assertion to exactly `string | undefined`. Regenerate with
  `mise run codegen:regen`, then run `mise run codegen:check` and
  `mise run ts:typecheck`. These are projection checks, not a new behavioral
  scenario or framework. Any native/source annotation changes receive the
  narrow appropriate Rust check; no runtime wire behavior should change.

## Verification limits

The review performed source tracing, base/current declaration comparisons,
AST signature comparison for moved Python declarations, and the focused
TypeScript counterexample. No Cargo build/test, regeneration, server journey,
or wheel import was run by this reviewer. Implementation evidence and the
orchestrator's independent successful lint/typecheck commands do not prove
response precision: the current compile fixtures either omit these assertions
or explicitly accept the widened shape. Unsupported future-schema hardening,
new API capabilities, and unrelated native signature debt were not reviewed.

One material shared-cause finding remains; no separate Python projection
finding was established.
