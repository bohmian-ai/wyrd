# Invariant review — FAIL

## Subject and authority

Repository: `/home/thorrester/Documents/GitHub/wyrd-worktrees/split/responses-input-forms`.
Original base: `807c4fe68ddcf43f325e970ed15f8a7c9918e0d4`.
Cumulative candidate: `100adbc04a39ca22022fb013b3c0e93bfe903c3a`, tree
`3bb993d4c32769cda67650eb02b991f981ebc3c2`. Original task is the approved
`changes/active/responses-input-forms/spec.md` revision 1, as directed by the
owner. Reviewed the cumulative diff and latest remediation, prior r1 verdict
and validated findings, and frozen `remediation-evidence.md`. This fresh review
did not read other r2 discovery reports, alter source, run generators, or
substitute the implementation summary for source.

Applied `AGENTS.md`, `architecture/agent-rules.md`, spec-driven-development,
maintainer-style, the relevant design/doctrine client and Skald boundaries,
security-posture, and routed Rust, PyO3, Python/stubs, TypeScript, testing,
agent-harness and error references. CodeGraph was the first code navigation
operation; subsequent source inspection expanded its map to gateway invocation,
adapter, existing result writers, generated consumers and test infrastructure.

## Cumulative acceptance matrix

| Requirement, criterion, constraint or non-goal | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| REQ-001 easy messages decode as the typed message they stand for | `openai_responses.rs:350-376`: only missing message discriminator and string message content are normalized before the existing tagged decoder | Exact `easy_input_messages_decode_as_typed_messages` independently rerun PASS | PASS |
| REQ-001 tool output accepts text or content parts and preserves the selected form | Untagged `OpenAiFunctionCallOutput` at `openai_responses.rs:470`; derived serialization retains Text versus Parts | Captured-body parts equality; existing text-output tool-loop coverage in supplied Skald lane | PASS |
| REQ-001 both caller forms reach provider unchanged | `routes.rs:1723-1738` discards the normalized temporary; `invoke_openai` retains original body; gateway adapter `native` overwrites only model | Real Agents SDK success test independently rerun PASS, asserting first easy input and second parts output at upstream | PASS |
| INV-001 tenant isolation | Parser handles input values only; unchanged `GatewayInvocation::admit_call` uses verified Caller tenant and permission decision before snapshot/admission/provider work | Exact under-privileged real agent test independently rerun PASS with 403 and no provider dispatch; surrounding source trace | PASS |
| AC-001 captured Agents SDK body unit | `openai_responses.rs:762-786` decodes easy user, typed assistant string, canonical function and parts-valued result together | Exact decoder selector independently rerun PASS | PASS |
| AC-002 format, lints, codegen and owning lanes | Existing manifest features and verification tasks; no new production dependencies or Cargo features | Independent fmt/diff check PASS; supplied final-pass lint/codegen/Skald/Python/TS evidence in frozen task | PASS within stated verification limits |
| Stronger repository real user-journey requirement, FIND-1 | Existing `serverTest`, WyrdTestServer, receiver, credentials and gateway deployments extended; actual `OpenAIResponsesModel`/Agent/tool drive two turns | Both exact success/refusal SDK tests independently rerun PASS | PASS |
| Accepted-input machine-readable parity, FIND-3 | `input_items_schema` on Items describes canonical item OR easy-message input, using existing content-part schema and named definition | Feature-enabled exact source schema unit and served OpenAPI test independently rerun PASS; supplied generation/typecheck PASS | PASS |
| Canonical output and other item invariants are preserved | No easy-message branch added to `OpenAiResponseItem` or response output; non-message tags remain governed by existing enum; only input schema selects easy alternative | Cumulative diff, generated declarations and served positive/negative shape assertions | PASS |
| Existing result writers keep text representation | `tool_result_message` and `Prompt::with_tool_result` change only String-to-enum construction through `From<String>` | Supplied Skald family PASS; source trace of both writers and loop expectation | PASS |
| Existing Python text accessor contract, FIND-2 | Accessor rustdoc at `python/openai_responses.rs:571-581` and authored/generated prompt stubs describe concatenation, omitted non-text parts and wrong-kind error | Static function/error-map/stub comparison; supplied codegen/typecheck PASS | PASS |
| No adjacent lifecycle/transport/tenancy or multimodal expansion | Existing decoder, wire enum, native adapter and test owners retained; no production auth, SQL, queue, timeout or cancellation owner changed | Complete 25-file cumulative diff and caller inspection | PASS |
| Rustdoc on every new Rust item | New `const EASY` in `input_items_schema` has no rustdoc | Direct source/diff inspection | FAIL — INV-R2-001 |

## Producer-to-consumer traces and existing alternatives

### Input normalization and raw forwarding

The Agents SDK produces an array with `{role:"user",content:"go"}`. The
Responses handler selects `GatewayResponsesRequest`, which transparently wraps
the existing Skald wire request. `OpenAiResponsesInput::Items` invokes
`easy_items`, inserts `type:message` only when type is absent and role exists,
and expands string message content into one text part. An assistant string
becomes `output_text`; other message strings become `input_text`. It then uses
the existing `OpenAiResponseItem::deserialize` for all required fields and
tagged variants. Missing role, invalid content and unknown tags remain errors;
no arbitrary-JSON fallback was added.

Typed normalization is not the gateway forwarding object. `typed_body` validates
against a borrow and returns its original JSON. `invoke_openai` moves that
original body into `GatewayCallRequest`; admission moves it into `AdmittedCall`.
The OpenAI/compatible Responses branch of `adapter::prepare` selects existing
`native`, which clones the JSON object and overwrites model only. The new real
journey checks these caller forms after that separate path. Replacing gateway
forwarding with serialization of the typed temporary would break the spec; the
candidate does not do so.

`OpenAiResponsesInput::items`, `items_mut` and `From<Vec<_>>` remain existing
normalization/editing owners for Prompt/Agent/Python consumers. Easy-message
round-trip fidelity inside the typed Skald value is not required: approved
behavior calls for its typed equivalent, and r1 validation explicitly preserves
that distinction. No second decoder or transport owner is justified or added.

### Tool-result representation and siblings

`FunctionCallOutput.output` now owns a closed Text/Parts value with untagged
Serde and the same content-part enum already used by messages. Deserialization
of a parts output therefore stores Parts, and serialization emits its original
array, not its text projection. Existing Agent result writing in
`request_builder.rs::tool_result_message` and Prompt writing in
`prompt.rs::with_tool_result` both use `From<String>` and keep their former
plain-text output. The existing Skald tool-loop test's expected result remains
a string with the same content; its constructor change is type adaptation.

Python `as_function_call_output` is the only new text-projection consumer found.
It borrows Text or concatenates input/output text in order for Parts and omits
non-text parts without mutating the retained item. Its wrong-kind path still
uses `wrong_variant`, and `PromptBuilderError::WrongVariant` maps to
`WYRD_PROMPT_400_PROVIDER_MISMATCH`, matching the new stub. No runtime, GIL,
transport or exception ownership was moved. FIND-2 is closed at its original
documentation boundary; no new accessor is required.

### Input contract producer and projections

The existing `OpenAiResponsesInput` was the deficient schema producer in r1.
The hook now asks the same Schemars generator for the canonical item and content
parts schemas, adds the named easy-message definition, and returns an item-list
union. Easy messages require role and content; content is string or existing
parts; type is absent or `message`. Canonical item decoding and output schema
do not acquire this input shorthand. The references use the generator's
configured definitions path, so both checked-in JSON Schema and served OpenAPI
component generation resolve the alternative through existing owners.

Reviewed the repeated schema changes across all nine owning schema artifacts
and both eval fixtures, generated TypeScript input and canonical item unions,
and Python dataclass/alias/decoder-table projections. Changes derive the same
input union and text/parts output contract rather than introducing another
semantic implementation. Python `from_wire` uses the generated tables; normal
server-returned Prompt data has already passed the Rust request decoder and is
canonical. Existing dataclass decoding is not a second easy-message parser.

The existing OpenAPI converter explicitly preserves composites and rewrites
definition references into components; the served test checks its assembled
result with the installed JSON Schema validator. This is stronger evidence than
checking only generated files. FIND-3 is closed at the shared producer, without
downstream guards or broadening canonical outputs.

## Prior finding closure

| Stable finding | Source-supported disposition |
|---|---|
| FIND-responses-input-forms-1 | CLOSED: actual Agents SDK two-turn tool journey and same-boundary denial use the existing real-server harness; current focused executions pass. |
| FIND-responses-input-forms-2 | CLOSED: original materially changed accessor and authored/generated stub now accurately describe projection and error; no behavior/signature change. |
| FIND-responses-input-forms-3 | CLOSED: input-only schema producer and generated declarations admit the easy variants; served contract proves conversion remains correct. |

## Proposed finding

### INV-R2-001 — VIOLATION: undocumented newly added schema-name constant

- Violated obligation: `AGENTS.md` §16 and `architecture/agent-rules.md` require
  substantive rustdoc on every new Rust item, explicitly including constants
  regardless of visibility. The invoked review skill preserves missing rustdoc
  as a hard blocker even without runtime consequences.
- Exact location: `crates/skald/skald-spec/src/wire/openai_responses.rs:315`.
- Source evidence: the remediation adds `const EASY: &str =
  "OpenAiEasyInputMessage";` directly inside `input_items_schema`, without a
  rustdoc comment. Its two downstream uses name the registered definition and
  construct the generated reference. The enclosing function has meaningful
  rustdoc, but the new constant item does not.
- Consequence: no runtime defect; the explicit per-item documentation
  completion gate is unmet on this new item.
- Smallest testable correction: document this existing schema-definition name's
  role in keeping registration and reference identity identical. Preserve the
  schema and existing tests; source inspection and the owning format check are
  sufficient closure. No new test, architecture choice or runtime refactor.
- Scope: a documentation correction only; all three prior findings remain
  closed. This is not a request to change the decoder, hook, output types,
  gateway lifecycle or test fixture.

## Schema unit versus served test

The source unit pins a named definition/reference structure used by generated
language declarations. The served test validates accepted/rejected requests
after Schemars-to-OpenAPI conversion and route assembly. They share a schema
producer but test different stages; the unit does not replace the served test.
Neither is a newly added repository check script or another check verifying a
check. The unit's exact structural assertions are not themselves an approved
product requirement, and deleting it is not needed to meet this task. No
finding or required test removal follows from their overlap.

## Verification limits

`r2/verification.md` records independent PASS for both exact Skald selectors,
served OpenAPI, both exact Agents SDK selectors, fmt and diff whitespace.
Frozen execution evidence records final PASS for broader lint, codegen, Skald
family and Python/TypeScript format/lint/typecheck lanes; those are supplied
results, not runs performed by this reviewer. No full aggregate, live-provider
lane, baseline checkout or pre-fix journey rebuild was run. Missing historical
journey RED is disclosed, but current source-supported behavioral closure and
focused passing executions are credible; no assertions/timeouts/skips were
weakened. Source was kept immutable; candidate consistency is owned by the
orchestrator's snapshot check.

Overall **FAIL** solely for INV-R2-001 under the explicit rustdoc rule; all
behavioral, tenant, schema, regression and prior-finding obligations pass.
