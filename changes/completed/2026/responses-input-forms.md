# Responses input forms through the gateway

- Change ID: `SPEC-responses-input-forms`, approved revision 1.
- Completed: 2026-10-14.
- Reviewed candidate: `100adbc04a39ca22022fb013b3c0e93bfe903c3a`.
- Completion target: `06a89286943a64c83eb588562b308c3d35663d66`, containing implementation commit `603f46d14`.
- Delivery reference: not supplied.
- Completion authority: explicit human override and official approval on 2026-10-14. No final-change-review PASS is claimed; the remaining local-constant rustdoc gate is waived for completion.

## Intent and implemented behavior

An OpenAI Agents SDK Responses agent can use the Wyrd gateway without a
client-side workaround for easy input messages or content-part tool results.
Easy messages such as `{"role":"user","content":"go"}` decode as their typed
equivalents. Function-call output accepts either text or content parts and
retains that selected form. The gateway validates the body and forwards the
original input to the provider, applying its existing model routing.

Published input schemas and generated Python/TypeScript authoring types now
describe easy messages beside canonical items. Canonical item and response
output types remain unchanged. The existing Python output accessor remains a
text view: it concatenates text parts without separators, omits nontext parts,
and retains its wrong-item-kind error.

## Lasting constraints and decisions

The existing Skald wire owner handles normalization and schema publication;
the gateway retains its existing authorization, tenant admission, provider
credentials, accounting and forwarding owners. Tenant isolation is unchanged.
Typed Skald normalization is distinct from raw gateway forwarding, so typed
objects need not preserve easy-message shorthand when serialized.

The real TypeScript Agents SDK tool journey reuses the existing server and
recording-provider infrastructure. Agents packages are test-only dependencies.
The source-schema unit protects named definitions used by authoring generators;
the served OpenAPI test protects payload acceptance after contract conversion.

Revision 1 is the approved behavior. Completion introduces no behavior or
architecture revision. The owner override accepts the remaining documentation
gate and completion without a new final integrated review.

## Acceptance evidence and limits

| Obligation | Evidence |
|---|---|
| REQ-001 and AC-001: typed easy input, text/parts output and captured SDK body | Exact decoder unit passed; existing Skald tests preserve text-result behavior. |
| REQ-001: original forms reach provider | Real Agents SDK tool journey passed through WyrdTestServer, asserting upstream easy input, content-part output and final answer. |
| INV-001: existing tenant and authorization boundaries | Independent source tracing found no changed authority; real scoped-reader journey passed with 403 and no upstream request. |
| Accepted input contracts | Exact schema unit and served OpenAPI positive/negative validation passed; generated declarations include easy input only. |
| AC-002: owning verification | Independent focused tests, format and whitespace checks passed. Implementation evidence records passing lint, codegen, Skald family and Python/TypeScript typing/format/lint lanes. |

The three original behavioral, contract and accessor-documentation findings
were independently closed. The remaining finding concerned rustdoc on a
function-local schema-name constant, with no runtime defect established.
Broader verification is recorded implementation evidence rather than repeated
closeout execution; no full integrated gate, live provider or release proof is
claimed. Completion performs document and packet validation only.

## Current authority, owners and tests

- [Wyrd design](../../../architecture/wyrd-design.md) and [security posture](../../../architecture/wyrd-security-posture.md).
- [Responses wire contract](../../../crates/skald/skald-spec/src/wire/openai_responses.rs), [gateway wire wrapper](../../../crates/wyrd-spec/src/gateway/openai.rs), and [gateway ingress](../../../crates/wyrd/wyrd-server/src/components/gateway/routes.rs).
- [Python accessor](../../../crates/skald/skald-prompt/src/python/openai_responses.rs), [generated Python types](../../../sdks/wyrd-sdk-python/python/wyrd/_card_types.py), and [generated TypeScript types](../../../sdks/wyrd-sdk-ts/wyrd/src/card-types.ts).
- [Real SDK gateway journeys](../../../sdks/wyrd-sdk-ts/wyrd/tests/integration/gateway-inference.test.ts), [served OpenAPI tests](../../../crates/wyrd/wyrd-server/tests/integration/pg_openapi_contract.rs), and [Skald Responses loop](../../../crates/skald/skald-agent/tests/integration/loop_responses.rs).
