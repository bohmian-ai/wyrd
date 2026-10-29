# Portable Verdicts and bounded structured-output repair

- Change: `agent-verdict-portability`
- Specification: `SPEC-agent-verdict-portability`, approved revision 2
- Completed: 2026-10-29
- Reviewed target: `f379283c5` (TASK-001 with remediation R1; base `911a8e278`); record committed on top as `cf57ff86d`
- Delivery reference: PR #222 (stacked on #220)

Agent judges now reach every provider with a Verdict schema it accepts, and a judge that answers with invalid structured output is corrected inside its own Agent budget instead of failing after the conversation ends. Invalid output still never becomes a judgment: a judge that never corrects itself leaves the requirement inconclusive with a stable code.

## Shipped behavior

- `Verdict::response_format` emits a portable projection: outcome is a string `enum`, locators are five closed shapes under `anyOf`, and generated nodes carry no `oneOf`, `const`, `minimum`, `format` or `$ref` siblings. An authored `data` schema is embedded afterwards, unchanged. Serialized Verdict values are unchanged.
- Every named format lowers to `strict: false` on OpenAI Chat and Responses; inline portable schemas keep strict mode.
- A structured-output Agent validates each proposed final answer against the exact schema its outgoing request sent, after variable binding and `before_model` hooks. Malformed JSON, non-objects and schema violations stay in the conversation, followed by a correction of at most 16 KiB listing up to ten violations with their rules, union alternatives and schema fragments. Each repair spends one of `max_iterations`, shared with tool turns.
- An answer still invalid on the last iteration, or a provider refusal without an answer (never repaired), ends the run with `AgentError::StructuredOutputInvalid`, public `WYRD_AGENT_422_STRUCTURED_INVALID` (HTTP 422), which is terminal and never retried. It replaces `WYRD_AGENT_422_STRUCTURED_DECODE`. Zero iterations and tool-only exhaustion keep `MAX_ITERATIONS`.
- Before an Agent check runs, Vala narrows that invocation's `citations[].slot` to the reachable original-evidence slots; with none reachable the schema is unchanged. Vala reports the new code terminally as inconclusive, keeps `verdict_from` typed, data and evidence guards, and no longer runs its own `conforms` validation. Code checks are not repaired.

## Lasting invariants

- Errors, public details and Vala reasons carry schema locations or fixed diagnostics only, never reply values, reply keys or parser text. Correction text is conversation content under the sensitive-capture policy.
- Narrowing never mutates Cards, authored serialization, hashes or concurrent invocations; Skald never knows the name `verdict`.
- Outcome values, closed objects, locator exclusivity, author data constraints, typed bounds and evidence checks stay enforced at their owners; dropped numeric annotations move rejection to typed decoding.

## Decisions and deviations

- New public error code and shared iteration semantics for repair were approved as the expensive decisions in revision 2, with the 16 KiB bound and the tool-only exhaustion rule.
- Named formats stay non-strict even without `data`, because live probes showed strict mode refusing author schemas with optional fields.
- No-source checks keep the unrestricted citation schema: empty enums and `maxItems: 0` are not portable across providers.
- Remediation R1 moved the schema authority from the Prompt to the outgoing request (`ResponseType::of`) and excluded refusals via `ResponseAdapter::refused`. Both are bounded corrections in the existing owners.
- The owner approved the change directly in place of a final `$wyrd-change-review`; full journey suites were not run at that point.

## Acceptance closure

| Criterion | Evidence |
| --- | --- |
| AC-001 portable envelope | `verification::execution::tests::plain_verdict_schema_is_portable` |
| AC-002 author schemas, OpenAI policy | `expanded_verdict_keeps_authored_data_schema`; `wyrd-skald-spec` named-format lowering tests |
| AC-003 repair within budget | `structured_output::*` (four encodings, bounds, exhaustion, caps, shared tool budget, bound and replaced schemas, refusals) |
| AC-004 per-call slots | `blocks_agent::check_agent_narrows_citation_slots_per_call`; `restrict_slots_admits_only_reachable_slots` |
| AC-005 safe errors, guards | `structured_output_exhaustion_is_safe`; `blocks_agent` code/redaction and refusal cases |
| AC-006 journeys | `judge_repairs_an_invalid_verdict` (Rust), Python and TypeScript building-block journeys, MCP `invalid_judge_answers_are_repaired_or_inconclusive` |
| AC-007 live smoke | `test_live_providers_accept_the_generated_verdict_schema` (OpenAI strict, Anthropic, Gemini) and `test_openai_chat_accepts_the_named_verdict_format`, each reply validated against the sent schema; 4/4 passed. Custom-data portability remains a documented limit. |

## Current owners

- [Wyrd design: Prompt, Agent, Verifier](../../../architecture/wyrd-design.md).
- [Verdict schema and slot narrowing](../../../crates/wyrd-spec/src/verification/execution.rs), [error catalog](../../../crates/wyrd-spec/src/error.rs).
- [Prompt and `ResponseType::of`](../../../crates/skald/skald-spec/src/prompt.rs), [response adapter](../../../crates/skald/skald-spec/src/adapter.rs), [named-format lowering](../../../crates/skald/skald-spec/src/authoring.rs).
- [Agent loop](../../../crates/skald/skald-agent/src/loop_runtime.rs), [structured validation](../../../crates/skald/skald-agent/src/structured.rs), [tests](../../../crates/skald/skald-agent/tests/integration/structured_output.rs).
- [Vala Agent blocks](../../../crates/vala/vala-verify/src/blocks/agent.rs), [tests](../../../crates/vala/vala-verify/tests/integration/blocks_agent.rs).
- Journeys: [Rust](../../../sdks/wyrd-sdk-rust/tests/integration/verifier_building_blocks.rs), [Python](../../../sdks/wyrd-sdk-python/tests/integration/test_verifier_building_blocks.py), [TypeScript](../../../sdks/wyrd-sdk-ts/wyrd/tests/integration/verifier-building-blocks.test.ts), [MCP](../../../crates/wyrd/wyrd-mcp/tests/bifrost/mcp/verification.rs); [live smoke](../../../sdks/wyrd-sdk-python/tests/smoke/test_gateway_live_providers.py).
- [Verifier Card reference](../../../docs/src/content/docs/reference/verifier-card.md).
