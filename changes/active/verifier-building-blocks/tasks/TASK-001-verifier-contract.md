---
id: TASK-001
kind: implementation
status: complete
spec: SPEC-verifier-building-blocks
spec_revision: 6
requirements: [REQ-001, REQ-002, REQ-003, REQ-004, REQ-005, REQ-006, REQ-007, REQ-009, REQ-010, REQ-011, REQ-012, REQ-013, REQ-014, REQ-015, INV-001, INV-003, INV-006, INV-008, AC-001, AC-009, AC-010]
depends_on: []
---

# One authoring and execution contract

## Outcome and Value

An author can construct, validate, and serialize the approved Verifier building
blocks and execution messages without a running server. The other three
parallel modules can implement against these same contracts without inventing
public fields, reference semantics, or duplicate validators.

## Owners, Scope, Consumers, and Prohibited Changes

Own pure Verifier, reference, input/result/status, binding, identity, and error
contracts in `wyrd-spec`; typed tool scope in its existing auth contracts and
resource/action/scope validation in `wyrd-runtime`; shared `MediaFormat` in `skald-spec`; and the minimum
engine-owned integration signatures needed by tasks 2 and 3. Put asynchronous
execution signatures at the existing Vala executor/invoker boundary, never in
`wyrd-spec`. No server endpoints, storage engines, scheduler, sandbox, or client
transport implementation belongs here.

The integration handoff must cover resolved values and their source slots,
action data versus check verdicts, explicit position and authority, deadline/
cancellation ownership, original citations, and step/check errors. Reuse the
existing injected executor/invoker capabilities; do not add a platform-wide
interface or prescribe private module structure. Runtime implementations
remain tasks 2 and 3's responsibility.

Keep the base buildable while consumers migrate: stage new contracts without
exposing an alternate live API or accepting two authored Verifier formats.
Existing types may temporarily remain solely for their existing consumers;
task 5 removes them and activates the replacement. Do not introduce aliases
or a compatibility feature. Record the exact staged exports and final switch
locations for tasks 2–5.

## Source-backed Reuse Map

| Capability | Existing owner/symbol | Inspected callers/tests | Missing behavior | Selected extension | New machinery justification |
| --- | --- | --- | --- | --- | --- |
| Authoring and binding | `wyrd-spec/src/card/verifier.rs`: `VerifierSpec`, `VerificationBinding`, `validate`; `vala/eval` DAG validation | `wyrd-server/src/components/cards/resolve.rs::check_activation`; `wyrd-spec/src/verification.rs::tests`; current example Cards | Implementation discriminator, kind-specific selectors, no named slots/requirements | Extend the foundational authoring and validation owner; reuse typed comparisons, references, and existing DAG validation | New block values are approved contracts; no second validator or graph store |
| Wire identity, errors, and results | `wyrd-spec/src/verification.rs`: `ExecuteVerificationRequest`, `Judgment`, `VerificationExecutionStatus`; `ids` and derive-backed `WyrdError` | HTTP verification routes and MCP projection; existing wire `tests` | Kind-specific input/result and separate run/direct IDs | Define approved generic messages, `ExecutionId`, complete status/error shapes, and async conflict outcome | Contract replacement required by spec; no new error envelope |
| Media | `skald-spec/src/media.rs::MediaRef`, `MediaKind`; `TenantMedia::resolve` in server `verification/eval.rs` | Prompt media binding; server media refusal tests | Free-form formats and no shared canonical sniff/format vocabulary | Add pure `MediaFormat` beside existing media types | One format vocabulary replaces string checks; no runtime IO in foundation |
| Extensible execution boundary | `vala-eval/src/executor.rs::TaskExecutor`, `Executors`; `judge.rs::JudgeInvoker`; orchestrator resolver traits | `execute_plan`, assertion/trace/judge executors; Vala integration target `orchestrator_judge_skald` | Separate data-producing actions and verdict-producing checks | Evolve the existing narrow injected execution seams; publish the shared compile contract | Multiple real action/check implementations earn the seam; no duplicate orchestrator |

Tool permission reuse: `wyrd-runtime/src/permission.rs::Permission`,
`Resource`, and `PermissionSet`, plus `wyrd-spec/src/auth/permission_scope.rs`,
already own typed scope validation, coverage, and intersection. Extend those
owners with REQ-008's tool resource/invoke action and exact-name scope; there
is no new permission store, checker, or string scope language.

## Approach

1. Revalidate the source map and approved r6 semantics; distinguish staged
   contracts from the live cutover owned by task 5.
2. Add the pure authoring vocabulary, scope/reference validation, explicit
   scheduled mappings, and one global block-ID namespace.
3. Add generic input, outcome, provenance, status, identity, and stable error
   contracts, including asynchronous replay-conflict reporting.
4. Complete media normalization and known-format compatibility rules using
   existing media and schema facilities.
5. Publish the minimum engine integration contract, compilation evidence,
   generated-contract inputs, and handoff notes for the parallel tasks.

## Ordered Implementation Scenarios

### Scenario 1 — A small Verifier can be authored and round-tripped

**Behavior.** No-step assertions and a preparation graph share one typed
contract, with all approved slot/action/check kinds and result shapes.
Publish every locked REQ-004 extraction output type and its discriminant,
required/null fields, ordering, and locator meanings; there are no redundant
counts or extracted-image helpers.
Defaults, finite positive thresholds, optional inputs, and empty requirements
have the meanings in REQ-001/002/005/006/015.

**RED.** Add the smallest IO-free serialization/schema/validation tests in the
owning library targets; show failures because the new authoring or wire shape
is missing. Collect actual names and run each exact focused selection before
implementation. The library commands below are the broader task proof.

**GREEN.** Implement the declarative values and validation, including explicit
schedule selector shapes and the existing error metadata pattern. Rerun this
scenario's tests after each behavior change.

**REFACTOR.** Consolidate duplicated pure validation in the existing contract
owner; keep Card envelopes declarative and runtime-free.

### Scenario 2 — Bad names and references fail before work starts

**Behavior.** Duplicate IDs across steps and requirements, cycles, unknown
references, inaccessible verdict references, invalid wildcard positions,
known-missing output fields, mixed Run/Window checks, and incompatible
slot/activation mappings are refused. Unknown output shapes remain legal and
runtime-missing values are representable as inconclusive (REQ-001–003/014).

**RED.** Add focused negative contract tests to the same libraries, one rule
at a time. Confirm the failure is the absent/refused validation behavior,
not an unrelated decoding or setup error.

**GREEN.** Reuse existing typed reference, visitor, and DAG capabilities for
these checks. Rerun scenarios 1 and 2; do not infer slot mappings from names.

**REFACTOR.** Remove duplicate validation logic while preserving precise safe
field/remediation details and the single global namespace.

### Scenario 3 — Agent and media contracts are position-safe

**Behavior.** Prompt placeholders/schema are derived rather than reauthored;
check Agents are tool-less; built-in tool names can be declared without
embedding runtime callables; original-slot citations survive ordinary derived
output; native media/extraction reject incompatible legacy/archive formats;
canonical formats round-trip without merging ppt with pptx (REQ-007/009/010).

**RED.** Add focused pure tests for position/schema/placeholder compatibility,
media token/MIME normalization, and incompatible bindings. Add exact
serialization/schema examples for every REQ-004 format, including the DOCX
block union and XLSX nullable values/formulas/errors. Do not test a
provider or launch a worker here.

**GREEN.** Extend existing shared media and Agent/Prompt validation. Rerun
all earlier scenario tests; the runtime remains injected and absent.

**REFACTOR.** Reuse one source of format/capability metadata and eliminate
parallel string vocabularies without adding dependencies to broad owners.

### Scenario 4 — Parallel modules share a usable boundary

**Behavior.** Engine, hosted checks, and clients can consume the published
contracts independently. Async conflicts fit existing execution error/status
semantics; no synchronous Postgres reservation is implied (REQ-011–015).

**RED.** Add focused compile/contract tests against the actual existing
executor/invoker and wire boundaries. Fail for missing data/verdict/provenance
or lifecycle distinctions; do not create a speculative mock platform.

**GREEN.** Complete the narrow shared signatures and typed messages. Record
which exports are stable for the parallel wave, and confirm existing callers
still compile or have the minimum staged adaptation.

**REFACTOR.** Remove unnecessary scaffolding. Hand off contracts and fixtures,
not empty production implementations or fake success paths.

### Scenario 5 — Tool scopes use Wyrd's general permission model

**Behavior.** REQ-008 applies Wyrd's existing global or exact scope permission
rule to tool invocation. Global and scoped tool-resource grants serialize,
validate, cover, and intersect through the existing permission model. Global
covers any registered built-in; exact covers only its named tool. Neither a
domain grant nor verifier invocation implies permission to execute tools.
Existing covering wildcard grants retain their established meanings.

**RED.** Add focused pure permission/schema cases for both approved wire
forms, invalid tool names, wrong resource/action/scope combinations, exact
name mismatch, disjoint scopes, and narrowed global/exact intersections.
Verify the new scope survives existing role and accepted-authority encoding
without becoming All. Record actual focused selectors in the owner crate.

**GREEN.** Extend the current auth contracts and runtime permission coverage
and intersection; preserve strict scope decoding and stable denial metadata.
Supply schema/contract changes to SDK and delegation consumers in tasks 4/5.

**REFACTOR.** Keep permission semantics in existing owners; remove duplicate
tool-name/permission validation rather than adding a second auth mechanism.

## Acceptance Criteria

- Authoring, validation, schema, and result examples match spec r6, including
  no applicable requirements, missing evidence, and delayed conflicts.
- Existing Card/Prompt loaders and reference visitors remain the source of
  resolution; no independent graph or validation subsystem appears.
- All three parallel tasks can consume the same typed boundary without
  deciding a new durable field or semantics.
- Foundation remains IO-/async-/PyO3-free; active old/new protocols do not
  coexist. Final obsolete-type removal is explicitly handed to task 5.

## Expected Write Set and Consumer Closure

`crates/wyrd-spec` authoring/verification/IDs/errors/reference visitors, auth
scope and schema sources; existing `wyrd-runtime` permission owners; `crates/skald/skald-spec/src/media.rs`; existing Vala executor/
invoker signature owners only where needed. Include pure tests and minimum
compile adaptations; mark staged live-consumer switch points. Shared generated
outputs are coordinated with task 4 and regenerated, never hand-edited.

## Verification and Evidence

TDD applies to validation, parsing, normalization, and changed logic. No
manufactured RED is needed for unchanged declarations or generated artifacts.
Use the repository toolchain; record actual exact test names/commands for each
RED/GREEN, without prescribing a new test-file structure here.

```bash
mise exec -- cargo nextest run --locked -p wyrd-spec --lib
mise exec -- cargo nextest run --locked -p skald-spec --lib
mise exec -- cargo clippy --locked -p wyrd-spec -p skald-spec --all-features --lib -- -D warnings
mise run fmt
mise run check:deps
```

Add focused permission-owner tests and compile/lint proof for changed
`wyrd-runtime` permission logic and Vala signatures. Do not run server
integration or full journey suites in this task. Their coverage is authored
by tasks 4/5 and executed after all five changes are merged (README).

## Material Stop Conditions

A required new Card kind, dynamic plugin API, public reference dialect, new
persistent/public field outside r6, or foundation runtime dependency requires
spec authority. Resolve reversible staging/export choices locally. Do not
unblock callers by accepting both old and new public formats.

## Authority Links

- [Approved specification r6](../spec.md), [supporting proposal](../proposal.md),
  [worked examples](../example.md), and [flow diagram](../architecture/verification-flow.html).
- [AGENTS.md](../../../../AGENTS.md), [agent rules](../../../../architecture/agent-rules.md),
  [Wyrd design](../../../../architecture/wyrd-design.md), and
  [Bifrost design](../../../../architecture/bifrost-design.md).
- [Execution and final proof coordination](README.md). Completed with owner approval
  at `a02055b5561b889fa9d934038109f3028b0ecb1d` on 2026-10-15.

## Implementation Evidence

Commits: `332743a58`, `2c69d954e`, `8df170581`, `4992a5111`, `1f01e9cee`,
`ab972411a`, `50a317fa8`, `6d9732040`, `6ff6f4a71`.

### Reuse map revalidation

| Capability | Existing owner reused | Gap closed | Extension |
| --- | --- | --- | --- |
| DAG planning | `vala::eval::plan::validate_dag` | Planning was tied to `EvalTask` | `plan_stages` generic over `(TaskId, deps)`; `validate_dag` delegates; `VerifierSpec::plan` calls it |
| Operator checks | `ComparisonOperator` + `spec.rs::validate_task_operator` | Regex check private to Eval | `ComparisonOperator::validate`, shared by both |
| Reference resolution | `refs::Visit` loader visitor | No visitor for new blocks | `impl Visit for blocks::VerifierSpec` |
| Validation errors | `WyrdError::Validation{message, details{rule, path}}` | — | Reused; no new error code, no codegen change |
| Media | `skald_spec::media` | Free-form format strings | `MediaFormat` beside `MediaRef`/`MediaKind` |
| Upload plans / digest / trace ID / route | `UploadPlan`, `Sha256Hex`, `TraceId`, `ExecutionRoute` | — | Reused in wire types |
| Status / verdict / error / drift | `VerificationExecutionStatus`, `VerificationVerdict`, `VerificationError`, `DriftReport`, `DriftWindow` | Error from `WyrdError` | `impl From<&WyrdError> for VerificationError` (`message_details` made `pub(crate)`) |
| Executor boundary | `TaskExecutor` pattern | Action data vs check verdict | `vala_eval::blocks::{ActionExecutor, CheckExecutor}` with their own two-variant `BlockError`; the LLM judge path is not kept (owner decision) and TASK-005 deletes it |

### Acceptance

| Acceptance criterion | Implementation evidence | Verification evidence | Result |
| --- | --- | --- | --- |
| Authoring, validation, schema, result examples match r4 (no applicable requirements, missing evidence, delayed conflicts) | `wyrd-spec/src/card/blocks/{mod,validate,reference,extract,schema_path}.rs`; `verification/execution.rs`; `skald-spec/src/media.rs`; `ids.rs` | `card::blocks::validate::tests::{verifier_spec_round_trips_and_validates, verifier_spec_pass_rate_bounds, verifier_spec_refuses_unknown_fields, verifier_spec_refuses_invalid_graphs_and_references, verifier_spec_enforces_agent_positions_and_media, verifier_spec_validates_schedule_inputs}`; `card::blocks::reference::tests::{reference_round_trips_every_root, reference_refuses_malformed_text, reference_serde_uses_string_form}`; `card::blocks::extract::tests::{extract_output_round_trips_locked_examples, extract_output_refuses_missing_and_extra_fields, extract_output_schema_selects_formats}`; `card::blocks::schema_path::tests::schema_path_refuses_only_known_missing_selectors`; `verification::execution::tests::{verification_request_round_trips_every_input_kind, verification_result_round_trips_documented_outcome, verification_error_carries_idempotency_conflict}`; `ids::tests::{slot_name_uses_identifier_grammar, execution_id_requires_uuid7}`; `media::tests::{media_format_parses_tokens_aliases_and_mime_types, media_format_serializes_canonical_tokens, media_format_keeps_legacy_office_distinct, media_format_native_kinds}` | PASS |
| Existing loaders and visitors remain the source of resolution; no independent graph/validation subsystem | `refs/mod.rs` `impl Visit for blocks::VerifierSpec`; `plan_stages`; `ComparisonOperator::validate` | `refs::blocks_tests::blocks_verifier_yields_every_reference_slot`; existing `vala::eval` plan/spec tests in the `wyrd-spec` lib run | PASS |
| Parallel tasks consume one typed boundary without new durable fields | `verification/execution.rs`; `vala-eval/src/blocks.rs` | `blocks::tests::action_executor_receives_resolved_inputs` (object-safe dyn dispatch, resolved inputs with source slots, deadline) | PASS |
| Foundation IO/async/PyO3-free; old/new protocols not both live | New wyrd-spec code is pure serde/schemars; async only in `vala-eval`; new types are not routed, registered, or reachable through `Spec` | `mise run check:deps` passed; no route/registry/codegen file touched | PASS |

### Commands (all passed)

```bash
mise exec -- cargo nextest run --locked -p wyrd-spec -p wyrd-skald-spec -p vala-eval --lib   # 1065 passed
mise exec -- cargo nextest run --locked -p wyrd-spec --lib -E 'test(/verification::execution::tests::/)'
mise exec -- cargo nextest run --locked -p vala-eval --lib -E 'test(=blocks::tests::action_executor_receives_resolved_inputs)'
mise exec -- cargo nextest run --locked -p wyrd-spec --lib -E 'test(=refs::blocks_tests::blocks_verifier_yields_every_reference_slot)'
mise exec -- cargo clippy --locked -p wyrd-spec -p wyrd-skald-spec -p vala-eval --all-features --lib --tests -- -D warnings
mise run fmt
mise run check:deps
git diff --check
```

The task's `-p skald-spec` is stale; the package is `wyrd-skald-spec`.
`codegen:check` was not run: no generator input, registered schema, or error
code changed.

### Staged exports and switch points

| Staged export | Consumer | Switch point (task 5 unless noted) |
| --- | --- | --- |
| `wyrd_spec::card::blocks::*` (`VerifierSpec`, `InputSlot`, `Step`, `Action`, `Check`, `Requirement`, `Decide`, `ScheduledInput`, `Reference`, `ExtractOutput`, `ResolvedAgent`) | Tasks 2–4 | Move to `card::verifier`, delete old `VerifierImplementation`/`VerifierTask`/Eval-verifier types, switch the `Spec` Verifier variant |
| `VerifierSpec::{validate, validate_with, plan, validate_schedule}` | Registry (task 5), engine (task 2) | Registration calls `validate_with` with the loader resolver; binding `inputs` field added at cutover and checked with `validate_schedule` |
| `wyrd_spec::verification::execution::*` | Server (task 5), SDKs (task 4) | Replaces `ExecuteVerificationRequest`, `Judgment`, `StartVerificationRunRequest`, and `/runs`; utoipa derives and route registration added then |
| `ExecutionId`, `SlotName` | All | `ExecutionId` replaces `VerificationExecutionId`/`VerificationRunId` |
| `skald_spec::MediaFormat` | Tasks 3, 4 | Replaces `TenantMedia::supported` string checks; content sniffing added by task 3 |
| `vala_eval::blocks::{ActionExecutor, CheckExecutor, BlockError, BlockCall, CallInput, CallValue, Position, ActionOutput, CheckOutput}` | Engine (task 2), hosted checks (task 3) | Engine drives them beside `execute_plan`; task 5 deletes the judge path (`JudgeInvoker`, `JudgeError`, `JudgeTaskExecutor`, `LlmJudge`) per its removal map |

### Handoff notes

- The engine owns deadlines and cancellation (races `BlockCall::deadline`,
  drops the future). Executors are built per execution with accepted caller
  authority; `Position` limits tools. Errors are `BlockError`
  (`Retryable` spends step retries; `Terminal` makes the block inconclusive).
  The LLM judge is not kept (owner decision): no new code depends on it, and
  the full removal map is in TASK-005's "Contract Handoff From TASK-001"
  (SDK/docs/fixture items in TASK-004's).
- `CallInput::sources` carries original slots so task 3 validates citations
  against reachable evidence; the engine downgrades a citation-less Agent pass.
- Deferred: `MediaFormat` content sniffing (task 3); validating `json` slot
  values against their schema at request time (task 5); utoipa/OpenAPI
  derives and registration (task 5).
- Left in place: `skald-prompt` `mime_from_extension` keeps its own table
  because its behavior differs from `MediaFormat` parsing.
- Non-goals respected: no endpoints, storage, scheduler, sandbox, transport,
  aliases, or compatibility feature.

### Rebase onto `change/deployment-addressing` (`08b38f742`)

Merged as `0f57c2c28`; textual overlap only in `skald-spec/src/media.rs` and
`wyrd-spec/src/error.rs`, both auto-merged. Silent conflict found: upstream
`Prompt` now records each portable `${media:name}` binding's block kind and
refuses a mismatched bind at runtime, which registration did not anticipate.
Fixed in `b269869a3`: `Prompt::media_kind` accessor; `native_media` refuses a
slot format whose native kind differs from the bound block. RED/GREEN:
`card::blocks::validate::tests::verifier_spec_enforces_agent_positions_and_media`.
Upstream `check_model`/`check_output_schema` accept every block fixture;
Upstream judge changes do not touch the block seams. After merge: lib suites 1069 passed, clippy clean,
`cargo check --workspace --all-features --tests` clean.

## Owner acceptance — 2026-10-15

The owner confirmed that the final remediation is complete and explicitly
approved Task 001 at `a02055b5561b889fa9d934038109f3028b0ecb1d`. This
includes the deployment-addressing merge `e258ce76c` and the final shared
selector correction in `a02055b55`. Findings 1–6 were closed by the preceding
independent review; the owner accepts remediation of FIND-TASK-001-7 using
the [R3 implementation evidence](../review/task-001-r2-20261015-542957a36/TASK-001-R3-schema-reference-presence.md#implementation-evidence).

Completion here records direct owner acceptance, not a new independent
review PASS. Prior immutable verdicts remain unchanged. The final remediation
evidence records 869 passing library tests, focused RED/GREEN, Clippy,
formatting, codegen and whitespace checks; those checks were not rerun for
this status update. Tasks 002–004 are ready to execute in parallel from this
shared handoff. Task 005 and final integrated proof remain outstanding.
