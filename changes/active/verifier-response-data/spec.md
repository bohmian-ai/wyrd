---
id: SPEC-verifier-response-data
revision: 1
status: approved
---

# Verdict formats and typed custom verification data

## Problem and proposal

Authors currently copy Wyrd's complete Verdict JSON Schema into Agent Prompts,
and code and Agent checks cannot return application-specific fields. Extend
[issue #200](https://github.com/bohmian-ai/wyrd/issues/200) with a generated
Verdict format and optional, declared custom `data`. Wyrd validates and retains
that data, and SDK callers can receive it as their own type.

This specification fixes the behavior and public contracts. The
[implementation task](tasks/TASK-001-verdict-formats-and-custom-data.md) explains
the code changes and their test-first execution order.

The ordinary form requires no copied schema or response-format prompt prose:

```yaml
response_format: verdict
```

The expanded form describes only the custom object. Inside `response_format`,
`data` is its schema; in the returned Verdict, `data` is its value:

```yaml
response_format:
  kind: verdict
  data:
    type: object
    properties:
      score:
        type: number
      category:
        type: string
    required: [score, category]
    additionalProperties: false
```

```json
{
  "outcome": "failed",
  "reason": "The answer disclosed restricted information.",
  "citations": [{"slot": "answer", "locator": {"json_pointer": "/text"}}],
  "data": {"score": 0.92, "category": "information_disclosure"}
}
```

This example assumes pinned JSON evidence in `answer.text`. Custom fields live
under `data` to avoid collisions with Wyrd fields. Each requirement result gets
its own optional `data`; the execution still has one aggregate verdict, without
an aggregate reason, citations, or custom-data object. Existing `actual` and
check-specific `detail`, including drift statistics, keep their meanings.

The costs are bounded extra payload and optional SDK decoding. Data is limited
to 64 KiB and 16 nested containers; larger outputs can be stored separately as
artifacts and referenced. A caller type applies to every present custom object
in a result, so mixed shapes need an appropriate union/decoder or ordinary JSON
reads.

## Required outcomes

The original outcome IDs below are retained; supporting requirements have been
consolidated under them without changing approved behavior.

### REQ-001 — Generate the standard Verdict format

`response_format: verdict` and `{kind: verdict}` generate the same closed
response from Wyrd's canonical Verdict type. Every terminal code or Agent check
returns required `outcome` (`passed | failed | inconclusive`), string `reason`,
and typed `citations` array. Citations may be empty subject to existing evidence
rules; SDK result representations may still omit an empty array.

An Agent declares the format on its Prompt, whether standalone, inline, or
referenced. Check usage never infers or replaces it, and a preparation Agent
may use the same explicitly declared format. Portable and native Prompts are
supported. Native authoring puts the named `response_format` beside `format`
and `body`; Wyrd compiles the provider settings, rejects simultaneous body-level
response settings, and preserves unrelated native options. Cards retain and
hash the authored format/schema, not expanded provider settings.

Existing compatible handwritten portable/native schemas remain supported.
Unknown names, invalid schemas, conflicting native settings, and incompatible
check contracts are refused before declaration persists, using existing
structured validation errors with field/remediation. Unsupported provider/schema
combinations use existing compiler errors; never weaken a schema silently.

### REQ-002 — Declare custom output for code and Agent checks

The expanded format accepts an object schema under `data`, including supported
nested objects, arrays, enums, and field constraints. Declaring it requires a
matching non-null object in the Verdict. Without a data schema, the Verdict
must not contain `data`. Unknown envelope members remain invalid. Handwritten
extended schemas must likewise explicitly declare and require their data object.

Agent authoring uses its Prompt's format. A code check accepts the same named
and expanded forms on its `check` object beside `kind: code`, source, entrypoint,
and inputs; omission retains the existing plain Verdict contract. The code
returns the envelope itself, without repeating its schema. Both whole-Verifier
and step requirements are covered. Preparation code actions remain arbitrary
JSON and do not gain this field.

Python and TypeScript authoring accept their already-supported model/schema
objects for `data`, avoiding a second handwritten schema. Rust accepts its
existing schema representation.

### REQ-007 — Validate and bound returned data

The server validates the entire response against the exact declared schema for
both check kinds; provider enforcement is insufficient. Malformed output,
missing/wrong-type custom fields, null or undeclared data, and unknown envelope
members make the requirement inconclusive through its existing failure path,
never a policy failure or pass. Rejected data is absent from the result.

| Bound | Inclusive rule |
| --- | --- |
| Size | At most 65,536 bytes of compact UTF-8 JSON for `data`, excluding the envelope and insignificant whitespace. No optional ASCII escaping; required JSON escapes count. |
| Depth | At most 16 containers. The root object counts as one; each nested object/array adds one; scalars add none. |

These are fixed server-owned limits, not Card/SDK options. Exceeding either
makes the requirement inconclusive, omits its data, and identifies the violated
limit and allowed bound without echoing contents. Never truncate or retain the
rejected object as requirement data. Existing provider, sandbox, transport, and
result bounds still apply; do not buffer unbounded output before validation.
Custom data never changes aggregation, evidence/citation validation, the existing
Agent pass/citation rule, code-check citation policy, dispatch, or execution
status, and never implies a score-to-verdict conversion.

### REQ-011 — Preserve data through existing result paths

Expose valid output directly as optional `RequirementResult.data` for both
check kinds and step requirements. Preserve it within the existing outcome in
`vala.verification.results` and through HTTP, MCP, and ordinary SDK results,
including delayed execution reads and published Bifrost storage. Old results
and checks without custom output omit the field. No new top-level Bifrost
column is required.

Data inherits the sensitive outcome's tenant isolation, authorization (the
results table's query-read permission and tenant scoping; the additional
gateway payload permission covers only gateway call payload columns, not
verification outcomes), retention, and existing durability/visibility
semantics. Do not copy it into lower-privilege metadata, logs, audit records, or
validation diagnostics, or repurpose `actual` and its capture rules. Existing
plain outputs, compatible handwritten schemas, and ordinary calls keep working;
new custom-data results require updated SDKs, without promising old-SDK support
for this new capability.

### REQ-012 — Return custom types automatically from SDK calls

Expose Python `VerificationResult[T]` and Rust/TypeScript
`VerificationResult<T>`, propagating `T` into each requirement's optional data.
Preserve standard types/access paths; omitted type parameters retain ordinary
JSON behavior. Callers need not subclass native results or replace the envelope.

Typed operations decode every present object before returning: Python accepts
a model exposing `model_validate` (including Pydantic); TypeScript accepts a
`parse` parser and infers its return type; Rust accepts Serde-deserializable
`T`. Model libraries remain optional. An annotation/cast alone is insufficient,
and caller types never change the server-declared schema.

| Operation | Python | TypeScript | Rust |
| --- | --- | --- | --- |
| Verify | `run.observe.verify_typed(verifier, data_type, /, ...)` | `run.observe.verifyTyped(verifier, inputs, parser, options?)` | `run.observe().verify_typed::<T>(verifier, inputs)` |
| Wait | `execution.wait_typed(timeout, data_type)` | `execution.waitTyped({timeoutMs}, parser)` | `execution.wait_typed::<T>(timeout)` |
| Status | `execution.status_typed(data_type)` | `execution.statusTyped(parser)` | `execution.status_typed::<T>()` |

Rust also has `wait_until_typed::<T>(deadline)`. Typed status converts only a
present result. These siblings share ordinary transport, authorization,
deadlines, attribution, and execution behavior. Python verifier/type arguments
to `verify_typed` are positional-only; existing input forms, `group_id`, and
input slots named `data_type` remain usable. Raw Bifrost SQL remains generic.

Absent data stays absent and is never passed to the decoder, including skipped
and inconclusive requirements. Decode all present data, including on failed
requirements. Any mismatch returns `WYRD_SDK_422_VERIFICATION_DATA_INVALID`
through the normal Wyrd error type, identifying the requirement and safe field
path without the payload. Return no partially typed result; change no stored
result/verdict and never rerun verification. Ordinary reads remain available.
This client decoding error is distinct from server-detected invalid check output.

For example, Python can return an actual model instance:

```python
from pydantic import BaseModel
from wyrd.verify import VerificationResult

class JudgeData(BaseModel):
    score: float
    category: str

result: VerificationResult[JudgeData] = run.observe.verify_typed(
    "policy-check", JudgeData, answer=text
)
data = result.requirements[0].get("data")
if data is not None:
    print(data.score)
```

## Scope and existing boundaries

Reuse the existing declaration, compiler, check execution, persistence, and
client workflows. `wyrd-spec` owns durable contracts/limits; existing Vala/server
owners validate and execute; SDK conversion is local. Named-format resolution
must preserve Skald's dependency direction: no dependency on Vala/server or
reverse dependency on `wyrd-spec`, and no duplicate compiler, orchestration,
transport, or result store. Existing [client](../../../architecture/wyrd-design.md#client-model),
[storage](../../../architecture/bifrost-design.md#storage-format-and-variant), and
[security](../../../architecture/wyrd-security-posture.md#tenant-and-data-isolation)
authorities continue to govern.

No new check kinds, arbitrary envelope fields, automatic artifact creation,
configurable limits, per-requirement decoder registries, result-query APIs,
inter-check data references, compatibility routes/aliases, or inferred formats.
Preparation actions do not become judgments.

Implementation must update the owning [Prompt](../../../architecture/wyrd-design.md#prompt)
and [Verifier](../../../architecture/wyrd-design.md#verifier) authority sections,
which currently describe JSON-Schema-only formats and closed three-field
Verdicts. Public SDK exports, Python stubs, generated TypeScript declarations,
runtime schemas, and HTTP/MCP descriptions must agree. Docs explain result
nesting and both check kinds in all three SDKs; the policy-judge example and
model-judge recipe stop copying Verdict schemas.

## Acceptance and evidence

| ID | Situation and expected observation | Outcome | Evidence |
| --- | --- | --- | --- |
| AC-001 | Execute plain named-format Agents without copied schemas/format prose; portable/native and inline/referenced forms agree; compatible handwritten formats still work. | REQ-001 | SDK/server journeys and compiler regression tests. |
| AC-002 | Both check kinds and step requirements return matching custom data; direct, queued wait/status, and retained/published reads agree. | REQ-002, REQ-011 | Rust, Python, TypeScript, HTTP, and MCP journeys through `WyrdTestServer` + repository Postgres and real storage readback. |
| AC-003 | Invalid names/schemas, native conflicts, and incompatible code/Agent declarations return field/remediation and persist no Card. | REQ-001, REQ-002 | Declaration journeys plus pure contract tests. |
| AC-004 | Missing/wrong/null/undeclared data and unknown envelope fields produce inconclusive requirements without rejected data; unsupported Agent passes still follow existing citation rules. | REQ-007 | Negative execution journeys for both check kinds. |
| AC-005 | Exact size/depth boundaries succeed; immediately larger values fail without truncation or payload diagnostics, including Unicode, JSON escapes, and wide arrays. | REQ-007 | Pure boundary tests plus execution journeys for both check kinds. |
| AC-006 | Every typed operation returns actual typed values, including failed requirements; absent data stays absent and heterogeneous decoders work. Mismatches return the exact SDK error; raw reads retain the original result without another execution. Python evidence named `data_type` remains usable. | REQ-012 | SDK runtime journeys, Python/TypeScript typechecks, and Rust compile-time coverage, including absolute-deadline wait. |
| AC-007 | Unauthorized/foreign-tenant reads gain no payload; typed reads preserve identities, verdicts, citations, provenance, and retained data. | REQ-011, REQ-012 | Authorization journeys and persisted-result comparisons. |
| AC-008 | Supported model/schema authoring needs no duplicate schema; public imports/types, docs/examples, and owning architecture match shipped behavior. | REQ-001, REQ-002, REQ-012 | Runtime authoring/export checks, generation, typechecks, and documentation verification. |

## Approval and revision history

No open material decisions. Revision 1 was approved by the maintainer's
instruction to create one implementation task after the draft handoff. Its
contracts include field placement, typed call names, limits, and failure
semantics. Approval does not approve tasks or assert implementation readiness.

| Revision | Status | Change |
| --- | --- | --- |
| 1 | approved | Named Verdict plus bounded custom data and typed SDK results. Editorial consolidation groups the original 16 requirements into five outcomes and references existing guarantees; approved behavior is unchanged. |

## Supporting references

[Repository rules](../../../AGENTS.md), [spec workflow](../../spec-driven-development.md),
[response/result contracts](../../../crates/wyrd-spec/src/verification/execution.rs),
[check declarations](../../../crates/wyrd-spec/src/card/verifier/mod.rs),
[declaration validator](../../../crates/wyrd-spec/src/card/verifier/validate.rs),
[Prompt compiler](../../../crates/skald/skald-spec/src/authoring.rs),
[Agent execution](../../../crates/vala/vala-verify/src/blocks/agent.rs),
[code execution](../../../crates/wyrd/wyrd-server/src/verification/hosted/sandbox.rs),
[result persistence](../../../crates/wyrd/wyrd-server/src/verification/results.rs),
[shared observe](../../../crates/shared/wyrd-client/src/observe/mod.rs),
[execution reads](../../../crates/shared/wyrd-client/src/verify/execution.rs),
[Python result stubs](../../../sdks/wyrd-sdk-python/python/wyrd/verify/__init__.pyi),
[TypeScript facade](../../../sdks/wyrd-sdk-ts/wyrd/src/index.ts), and
[Rust exports](../../../sdks/wyrd-sdk-rust/src/lib.rs).
