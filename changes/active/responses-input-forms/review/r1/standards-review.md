# Repository standards review

## Subject and result

**FAIL.** Reviewed base `807c4fe68` through immutable candidate
`6294bf6bbc0acb9abc37e4e4d617e2638f9dc3f7`. The approved specification is
also the original task, as explicitly directed by the user. No separate task
packet is required. No source was changed in this audit.

The material findings are a missing required gateway journey and missing
rustdoc on a materially changed Python projection. These are proposed findings
for independent validation, not final finding IDs.

## Authority coverage

The reference router is `architecture/references/README.md`. Governing
authorities are `AGENTS.md`, `architecture/agent-rules.md`,
`architecture/wyrd-design.md`, `architecture/wyrd-doctrine.mdx`, and the
applicable gateway trust/identity requirements in
`architecture/wyrd-security-posture.md`. Selected references are
`doctrine/architecture-constraints.md`, `architecture/patterns.md`, and
`languages/{spec-driven-development,maintainer-style,rust-core,pyo3-boundaries,python-api-and-stubs,typescript-guide,testing-workflows}.md`.
Bifrost ingest/query/publication and operational state are not changed.

| Changed surface | Applicable rules and reference coverage | Source inspected |
|---|---|---|
| Skald Responses input parser and new output enum | AGENTS §§2–6, 10–12, 15–16; agent-rules pure parsing, struct ownership, rustdoc; Rust core, patterns, architecture constraints | `crates/skald/skald-spec/src/wire/openai_responses.rs`: `OpenAiResponsesInput`, `easy_items`, `OpenAiResponseItem`, `OpenAiFunctionCallOutput`, implementations and inline tests; owning manifest |
| Agent tool-result authoring | AGENTS §§3–6, 10, 16; Rust core and ownership/patterns | `crates/skald/skald-agent/src/request_builder.rs::tool_result_message` and callers identified by CodeGraph; unchanged surrounding provider branches |
| Prompt tool-result authoring | AGENTS §§3–6, 10, 16; Rust core and patterns | `crates/skald/skald-prompt/src/prompt.rs::Prompt::with_tool_result` and owning manifest |
| Existing Python item projection | AGENTS §§7–8, 12, 16; PyO3 boundaries, Python API/stubs, Rust documentation | `crates/skald/skald-prompt/src/python/openai_responses.rs::PyOpenAiResponseItem`, `src/python/mod.rs` registration, Python SDK `src/lib.rs:102`, public prompt exports and generated `.pyi` |
| New inline test and mechanically adapted tool-loop fixture | AGENTS §11; agent-rules test placement/runtime ownership; testing workflows | New `easy_input_messages_decode_as_typed_messages`; `crates/skald/skald-agent/tests/integration/loop_responses.rs` helper/tool and agent loop; Python prompt tests; TypeScript gateway integration tests |
| Nine Wyrd schemas and two eval schema fixtures | AGENTS §§2, 8–9, 11–12; agent-rules generated-artifact ownership; patterns and testing workflows | Complete cumulative diff: `agent_spec`, `card`, `eval_spec`, `get_card_response`, `prompt_ref`, `prompt_spec`, `verifier_implementation`, `verifier_spec`, `workflow_spec`; eval `eval_spec.schema` and `eval_task.schema` fixtures |
| Python generated Card types | AGENTS §§8–9, 11–12; Python API/stubs and generated-contract parity | `_card_types.py` new alias, output dataclass field, `_OBJECTS`, `_ALIASES` |
| TypeScript generated Card types | AGENTS §§2–3, 9, 11–12; TypeScript guide and generated-contract parity | `card-types.ts` new alias and `OpenAiResponseItemFunctionCallOutput.output` |
| Active specification | AGENTS §14 and spec-driven development, subordinate to repository authority | `changes/active/responses-input-forms/spec.md`; user instruction makes it the original task |
| Gateway consumer of changed parser | AGENTS §§2, 9–11; security trust boundary, architecture constraints/patterns | CodeGraph caller path: `routes.rs::responses` → `typed_body::<GatewayResponsesRequest>` → existing `openai_call` / `invoke_openai`; no handler, authorization, SQL capability, or transport diff |

## Rule results

| Applicable obligation | Result | Evidence |
|---|---|---|
| Provider-specific wire behavior stays in Skald; pure contract code remains IO/PyO3/async-free | PASS | New parser and output enum are in `skald-spec`; manifest already owns serde/serde_json; no dependency, feature, runtime, or server-owner changes |
| Struct-centered shape with pure helpers permitted; synchronous parsing | PASS | `easy_items` is a synchronous deterministic serde hook with invocation-owned values; output behavior is an inherent method on its domain enum; no retained-resource owner is introduced |
| Trust-boundary decoding uses typed shapes and propagates failure | PASS | Decoder delegates each normalized item to `OpenAiResponseItem::deserialize` and maps its error; no unchecked unwrap, external IO, bypass, or new public error mapper |
| SQL capability/tenancy and blocking permission/nonblocking audit boundaries | PASS for changed paths | No SQL signatures/fields or audit/permission implementation are changed. Parser has no tenant state; existing gateway caller and service remain the owners. This is a bounded source judgment, not a claim that a generic SQL scanner proves all paths |
| Existing optional-feature Python migration boundary remains thin | PASS | Existing `PyOpenAiResponseItem` calls Rust `output.text()`; no new class, Python lifetime, runtime, transport, validation owner, or relocation |
| Python registration/exports/signature parity | PASS | Existing class is registered through `skald_prompt::register_prompt`; public `wyrd.prompt` stub keeps `as_function_call_output() -> str`, matching Rust return. Generated Card shape adds the union alias in field and decode tables |
| TypeScript/schema projection of newly added output union | PASS | Every changed schema references `OpenAiFunctionCallOutput`; Python and TypeScript generated projections use string-or-content-parts; original field names retained |
| Generated artifacts regenerate cleanly | PASS | Orchestrator reports `mise run codegen:check` exit 0 in disposable archive of the exact candidate; reviewed source remained immutable; `codegen-check.log` and `verification.md` contain evidence |
| New pure logic has credential-free owning-module coverage | PASS | Inline test decodes captured request shape and asserts typed easy-message conversion plus preserved tool-output parts; exact nextest command selects one test and passes |
| External Rust fixture earns separate binary; language-runtime ownership maintained | PASS | Existing loop test wires Skald agent, prompt, runtime and tool crates; no Python or Node lifetime is initialized in Rust tests; adapted `.into()` preserves string-output fixture semantics |
| User/agent-facing capability ships real client → server → client journey | FAIL | `STD-001` below |
| Materially changed Rust item has substantive rustdoc and fallible method `# Errors` | FAIL | `STD-002` below |
| Format, lint and owning Rust lane | PASS | Orchestrator reports format check, lints, focused unit test, and `test:skald` (345 passing tests) all passed; see `verification.md` |
| No weakened checks, test deletion/ignore, suppressions, legacy routes, aliases or names | PASS | Complete diff adds one inline test, mechanically updates one fixture; no check/test gates or suppression attributes changed |

The `.into()` edits in `tool_result_message`, `Prompt::with_tool_result`, and
the existing loop fixture preserve their prior string-output behavior. They
are mechanical type adaptations, not newly invented semantic operations; this
review does not demand unrelated documentation rewrites on those symbols.

## Proposed material findings

### STD-001 — Required gateway journey is absent

- **Classification:** MISSING / repository test obligation.
- **Rule:** `AGENTS.md` §11; `architecture/agent-rules.md` user-journey rule;
  `architecture/wyrd-design.md` Doctrine 20; testing-workflows “Three Tiers”.
- **Location:** `changes/active/responses-input-forms/spec.md:52` explicitly
  records the unwritten TypeScript Agents SDK gateway journey. New proof is
  limited to `crates/skald/skald-spec/src/wire/openai_responses.rs:691`.
- **Evidence:** The new inline unit test calls serde directly. The adapted
  Skald loop uses `MockProvider`, not the gateway. Existing
  `sdks/wyrd-sdk-ts/wyrd/tests/integration/gateway-inference.test.ts` calls
  Chat Completions, not `OpenAIResponsesModel`; the scanned TypeScript
  integration tree contains no Responses Agents SDK tool-loop journey.
- **Consequence:** Real agent ingress, authorization, request preservation,
  provider relay and second-turn tool-output acceptance are unproven together.
  The unit assertion even intentionally canonicalizes easy messages and
  drops extra function-call fields, so it cannot stand in for proving the
  gateway's raw forwarded body.
- **Smallest testable correction:** Extend the existing TypeScript
  `serverTest`/local-upstream gateway integration owner with an actual Agents
  SDK `OpenAIResponsesModel` tool round trip through `WyrdTestServer`. Assert
  upstream receives the easy input and content-part tool result unchanged
  (apart from existing gateway routing/model behavior), and verify denied
  caller and malformed-body refusal without upstream dispatch at this same
  boundary. Reuse existing fixture and narrow integration lane; do not add
  new harness machinery or require unrelated suites.
- **Authority qualification:** The approved spec acknowledges a gap; it does
  not explicitly override stronger repository journey requirements. Whether
  the approval constitutes an intentional waiver must be resolved from
  authority by independent validation, not inferred from a passing unit test.

### STD-002 — Changed Python projection lacks required rustdoc

- **Classification:** VIOLATION, hard documentation gate.
- **Rule:** `AGENTS.md` §16 and agent-rules Rust documentation rule require
  substantive rustdoc on materially modified private methods, including
  `# Errors` on fallible ones.
- **Location:**
  `crates/skald/skald-prompt/src/python/openai_responses.rs:571`,
  `PyOpenAiResponseItem::as_function_call_output`.
- **Evidence:** The entire method has no rustdoc. Its behavior changes from
  cloning a single string to returning concatenated input/output-text parts
  while skipping nontext parts through `OpenAiFunctionCallOutput::text()`.
  The non-function-call-output branch still returns a catalog-backed
  wrong-variant error.
- **Consequence:** The Python accessor's newly lossy text projection and
  failure conditions are absent from its source contract. This independently
  violates the repository's explicit merge-blocking rustdoc requirement.
- **Smallest testable correction:** Add rustdoc on this modified method
  explaining the string/parts projection, text concatenation, nontext-part
  omission and `# Errors` for wrong variant. Keep the existing Rust owner and
  Python return type. Regenerate dependent public documentation/stubs if
  emitted by the existing generator and check drift; no new API or wrapper.

## Verification limits

All available orchestrator format/lint/Skald/codegen results pass. This
specialist ran no additional compile, generator, live provider, Python or
TypeScript execution and changed no implementation. Static coverage includes
their changed generated and PyO3 surfaces; the missing real gateway journey
remains the principal runtime proof gap. No new security or tenancy defect
is proposed from the unchanged server machinery.
