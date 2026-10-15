---
id: TASK-001-R1
kind: remediation
status: proposed
spec: SPEC-codegen-single-source
spec_revision: 1
parent_task: TASK-001
remediates: [FIND-TASK-001-1]
requirements: [REQ-007, INV-001, INV-003, INV-004, INV-005, AC-006]
depends_on: []
---

# Preserve Rust serialization shapes in TypeScript responses

Required implementation skill: `$wyrd-implement`. This is bounded remediation
under existing approval, not a new design plan or another scenario matrix.

## Authority and reviewed candidate

- [Approved spec revision 1](../../spec.md).
- [Original TASK-001](../../tasks/TASK-001-consolidate-sdk-types-and-codegen.md).
- [Review verdict](verdict.md) and [independently validated ledger](findings-validation.md).
- Original base: `5bcd9e4872f1376f1722922fa63b67384243be04`.
- Reviewed candidate: `eeac9c9a45b599b1727086ba30582d3f02b3230a`.
- Finding: **FIND-TASK-001-1**, consolidating BEHAVIOR-001, INVREV-001, SDK-001.

## Diagnosis

REQ-007 requires generated optionality/nullability to match Rust and INV-003
preserves public precision. The existing lowerer at
`scripts/gen_ts_types.py:179–190,205–225` sees default schemars 0.8 metadata
describing accepted input, then approximates output presence from schema
required/default fields. That metadata has already lost serialization rules.

The actual output path is Rust response DTO → native
`NativeLifecycleResult::outcome` JSON serialization → existing TypeScript
wrapper → generated wire declaration. Wrappers do not remove or add these keys.

- `WorkflowRun`/`WorkflowStepResult` in `wyrd-spec/src/card/workflow.rs:775–831`
  have unskipped Option fields. Rust always emits keys such as `started_at`,
  `ended_at`, `text`, and `error`, using null for None. Generated optional
  properties instead add `undefined`, breaking a previously valid assignment
  such as `const started: string | null = run.started_at`.
- `BifrostTableDescription` in `wyrd-spec/src/vala/api.rs:129–145` skips absent
  fingerprint/compaction fields. Its scalar fields are non-null when present,
  but generation admits null. The changed table-description compile fixture
  explicitly accepts that widening.
- Registration/listing, principal/credential metadata, provider credential
  views, verification judgments and shared layout descendants pass through
  the same producer. Fixing Workflow or Bifrost at the consumer would leave
  siblings incorrect and recreate duplicated contract authority.

The validator independently reproduced unexpected-undefined TS2322 diagnostics
and an unused expected-error TS2578 for the accepted null. Existing lint and
type lanes pass because their fixtures do not preserve these guarantees.
Acknowledging this approximation in implementation evidence does not authorize
it; INV-005 also precludes weakening an accurate fixture to accommodate it.

## Correction outcome and selected approach

Retain the existing Rust contract owners, extraction example, lowerer and
generated module. Supply accurate serialization metadata at the Rust owner and
consume it in the existing output projection. Keep input and output semantics
separate where a Rust definition serves both.

Use the installed mechanisms selected by independent validation:

1. **Always-emitted ordinary Option fields:** express their existing default at
   the Rust declaration with `#[serde(default)]` or the equivalent existing
   schema annotation mechanism. For these fields, missing input already means
   None and serialization remains value-or-null with an always-present key.
   The metadata exposes `default: null` to the current output rule. Preserve
   existing field schema overrides, including URL projections. Do not default
   required non-Option input fields.
2. **Response-owned skipped Option fields:** retain serde omission/default
   behavior and describe the emitted underlying type with existing schemars
   annotations. Scalar/reference values are optional and non-null when
   present. For `Option<serde_json::Value>`, underlying JSON still permits
   null: Some(Null) is serialized, so do not erase valid nulls.
3. **Shared input/output fields:** preserve their normal input schema and
   attach source-owned serialized-value schema metadata using the existing
   `schema_with`/`SchemaObject.extensions` facilities. Consume that metadata
   only in the existing output-mode lowerer. In particular,
   `PhysicalLayoutWire.time_column` also belongs to `RegisterTableRequest`
   and durable layout JSON; replacing its ordinary schema with String would
   improperly narrow accepted nullable/omittable inputs. Reuse the existing
   `wyrd_spec::schema` utility owner and custom schema precedent in
   `auth/revoke.rs::reason_schema`; no second generator or public DTO family.

The installed schemars 0.8.22 has no serialization-contract setting. Do not
invent a settings switch, globally require nullable fields, or use
`#[schemars(required)]` as a blanket fix: its derive strips Option nullability.
A visitor over the current lossy bundle alone cannot recover the missing fact.
The correction remains one Rust-owned contract projection; no field-name lists,
generated-file hand edits, SDK interface copies, or downstream guards.

## Owners and sibling closure

Apply the same serialization rule through the existing response roots and
reachable definitions identified by validation:

- WorkflowRun and WorkflowStepResult.
- CardRegistrationOutcome, CardSummary and ListCardsResponse, with their
  RegistrationReceipt consumer.
- CredentialMetadata, PrincipalSummary, PrincipalPage and ProviderCredentialView.
- QueryTerminalFrame.error.
- Judgment and reachable FeatureDriftReport, SkippedTask and AssertionResult.
- BifrostTableDescription fingerprint/compaction fields; shared
  PhysicalLayoutWire.time_column; DataTypeSpec's Timestamp `tz`, whose
  unskipped Option serializes as a required nullable field.

These are the same producer/correction boundary, not separate tasks or journeys.
Paths/definitions guide consumer closure and do not prescribe private helpers
or local module structure. Preserve the already-correct `FieldSpec.metadata`
defaulted/skipped map as optional and non-null. Keep other skipped collections,
shared Card/ref/spec authoring acceptance, native serialization and TS wrapper
behavior intact. Do not bundle unrelated preexisting layout refinements.

Likely write set: owning Rust contract/schema metadata, the current extraction
or lowerer only where necessary to carry/consume that metadata, regenerated
`wyrd/src/card-types.ts`, and existing compile-only fixtures. No dependency or
feature change is authorized.

## Acceptance criteria

All criteria close **FIND-TASK-001-1**:

- Required nullable workflow fields have exactly their actual output types,
  including `WorkflowRun.started_at` and `WorkflowStepResult.text` as
  `string | null`, without `undefined`.
- Fingerprint/other skipped scalar outputs are optional and exclude impossible
  null; opaque JSON preserves representable null values.
- Representative registration/principal response fields and identified sibling
  output roots follow the same source-owned rules.
- Shared layout inputs retain null/omission acceptance while their skipped
  output values use the underlying serialized type. Existing skipped metadata
  map optionality/nullability is unchanged.
- No runtime JSON, request requiredness, default behavior, validation, error,
  auth, tenancy, persisted format, Card authoring, or wrapper API is changed.
- Existing type fixtures protect these guarantees instead of accepting the
  approximation; surviving generated output reproduces and scoped checks pass.

## Focused proof and evidence

Strengthen the existing compile-only fixtures first. The current candidate
must fail for unexpected undefined and/or incorrectly accepted null, then pass
after source correction and regeneration. Reuse the workflow, Card/registry
and table-description type fixtures; do not add a behavioral scenario matrix.
Use a small ordinary owner-level projection check for the shared metadata
branch that preserves input nullability, opaque JSON and skipped-map behavior.
Record any newly named test with its exact focused command when its owning
target/name exists; do not invent a selector or testing framework.

Exact compile-fixture iteration command:

```bash
mise exec -- pnpm --dir sdks/wyrd-sdk-ts/wyrd exec tsc --noEmit
```

Focused final checks for this correction:

```bash
mise run codegen:regen
mise run codegen:check
mise run ts:typecheck
mise run ts:lints
mise run fmt:check
mise exec -- cargo clippy --locked -p wyrd-spec --all-targets --all-features -- -D warnings
```

When the root Python lowerer changes, its actual file also requires checks
outside the SDK-only Python lint task:

```bash
mise exec -- uv run --project sdks/wyrd-sdk-python ruff check scripts/gen_ts_types.py
mise exec -- uv run --project sdks/wyrd-sdk-python ruff format --check scripts/gen_ts_types.py
```

Regeneration compiles the schema example. Record the focused counterexample,
GREEN output, preserved input/output distinctions and exact command results.
Do not add broad journey, docs or gate runs to this bounded remediation;
integrated change review owns the final aggregate.

## Constraints and stop conditions

Preserve the original spec's source authority and non-goals. No new dependency,
feature, extractor, generic generator framework, schema tree, public runtime
type family, consumer guard, copied field list, or handwritten wire interface.
Do not alter production serializers merely to match a schema approximation.

If the correction proves to require a material contract/ownership/dependency
change, stop for specification authority. Ordinary source metadata, private
schema/lowering support, type-fixture placement, exact test naming and in-scope
lint repairs remain implementation-owned.

After remediation, request independent task review of the complete original
base-to-new-candidate range, preserving FIND-TASK-001-1 and this diagnosis.
